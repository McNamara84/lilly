use std::collections::HashSet;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use image::codecs::jpeg::{JpegDecoder, JpegEncoder};
use image::codecs::png::PngDecoder;
use image::codecs::webp::WebPDecoder;
use image::imageops::FilterType;
use image::{DynamicImage, GenericImageView, ImageDecoder, ImageFormat, Rgb, RgbImage};
use rand::RngExt;

use crate::config::PhotoUploadConfig;
use crate::db::media as media_db;
use crate::models::media::ProcessedPhoto;

const USER_PHOTO_DIRECTORY: &str = "user-photos";
const TEMP_DIRECTORY: &str = ".tmp";

#[derive(Debug, thiserror::Error)]
pub enum MediaError {
    #[error("Photo exceeds the configured upload or decoding limit")]
    TooLarge,
    #[error("Unsupported image format; use JPEG, PNG, or WebP")]
    Unsupported,
    #[error("Invalid or malformed image")]
    Invalid,
    #[error("Photo storage failed")]
    Storage(#[source] std::io::Error),
}

#[derive(Debug)]
pub struct StagedPhoto {
    pub storage_key: String,
    temporary_path: PathBuf,
    final_path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct MediaStorage {
    root: PathBuf,
}

impl MediaStorage {
    #[must_use]
    pub fn new(media_path: &Path) -> Self {
        Self {
            root: media_path.join(USER_PHOTO_DIRECTORY),
        }
    }

    pub async fn prepare(&self) -> Result<(), MediaError> {
        tokio::fs::create_dir_all(self.root.join(TEMP_DIRECTORY))
            .await
            .map_err(MediaError::Storage)
    }

    pub async fn stage(&self, bytes: &[u8]) -> Result<StagedPhoto, MediaError> {
        self.prepare().await?;
        let storage_key = generate_storage_key();
        let temporary_path = self
            .root
            .join(TEMP_DIRECTORY)
            .join(format!("{storage_key}.upload"));
        let final_path = self.path_for_key(&storage_key)?;
        let mut options = tokio::fs::OpenOptions::new();
        options.write(true).create_new(true);
        let mut file = options
            .open(&temporary_path)
            .await
            .map_err(MediaError::Storage)?;
        if let Err(error) = tokio::io::AsyncWriteExt::write_all(&mut file, bytes).await {
            let _ = tokio::fs::remove_file(&temporary_path).await;
            return Err(MediaError::Storage(error));
        }
        if let Err(error) = tokio::io::AsyncWriteExt::flush(&mut file).await {
            let _ = tokio::fs::remove_file(&temporary_path).await;
            return Err(MediaError::Storage(error));
        }
        drop(file);
        Ok(StagedPhoto {
            storage_key,
            temporary_path,
            final_path,
        })
    }

    pub async fn commit(&self, staged: &StagedPhoto) -> Result<(), MediaError> {
        tokio::fs::rename(&staged.temporary_path, &staged.final_path)
            .await
            .map_err(MediaError::Storage)
    }

    pub async fn discard_staged(&self, staged: &StagedPhoto) {
        // Never remove the final file here: after a database commit error the
        // commit may still have reached MariaDB. Keeping the canonical file is
        // safe in both outcomes; startup reconciliation removes it as an
        // orphan only when no photo row exists.
        let _ = tokio::fs::remove_file(&staged.temporary_path).await;
    }

    pub async fn read(&self, storage_key: &str) -> Result<Vec<u8>, MediaError> {
        let path = self.path_for_key(storage_key)?;
        tokio::fs::read(path).await.map_err(MediaError::Storage)
    }

    pub async fn remove(&self, storage_key: &str) -> Result<(), MediaError> {
        let path = self.path_for_key(storage_key)?;
        match tokio::fs::remove_file(path).await {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(MediaError::Storage(error)),
        }
    }

    fn path_for_key(&self, storage_key: &str) -> Result<PathBuf, MediaError> {
        if !is_valid_storage_key(storage_key) {
            return Err(MediaError::Invalid);
        }
        Ok(self.root.join(storage_key))
    }

    async fn remove_temporary_files(&self) -> Result<(), MediaError> {
        let temporary_root = self.root.join(TEMP_DIRECTORY);
        let mut entries = match tokio::fs::read_dir(&temporary_root).await {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(MediaError::Storage(error)),
        };
        while let Some(entry) = entries.next_entry().await.map_err(MediaError::Storage)? {
            if entry
                .file_type()
                .await
                .map_err(MediaError::Storage)?
                .is_file()
            {
                let _ = tokio::fs::remove_file(entry.path()).await;
            }
        }
        Ok(())
    }

    async fn remove_orphans(&self, active: &HashSet<String>) -> Result<u64, MediaError> {
        let mut removed = 0_u64;
        let mut entries = match tokio::fs::read_dir(&self.root).await {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
            Err(error) => return Err(MediaError::Storage(error)),
        };
        while let Some(entry) = entries.next_entry().await.map_err(MediaError::Storage)? {
            if !entry
                .file_type()
                .await
                .map_err(MediaError::Storage)?
                .is_file()
            {
                continue;
            }
            let key = entry.file_name().to_string_lossy().into_owned();
            if is_valid_storage_key(&key) && !active.contains(&key) {
                tokio::fs::remove_file(entry.path())
                    .await
                    .map_err(MediaError::Storage)?;
                removed += 1;
            }
        }
        Ok(removed)
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct ReconciliationStats {
    pub completed_jobs: u64,
    pub failed_jobs: u64,
    pub removed_orphans: u64,
}

pub async fn reconcile_storage(
    pool: &sqlx::MySqlPool,
    storage: &MediaStorage,
) -> Result<ReconciliationStats, anyhow::Error> {
    storage.prepare().await?;
    storage.remove_temporary_files().await?;
    let mut stats = ReconciliationStats::default();
    for job in media_db::pending_deletion_jobs(pool, 1_000).await? {
        match storage.remove(&job.storage_key).await {
            Ok(()) => {
                media_db::mark_deletion_processed(pool, job.id).await?;
                stats.completed_jobs += 1;
            }
            Err(error) => {
                media_db::mark_deletion_failed(pool, job.id, &error.to_string()).await?;
                stats.failed_jobs += 1;
            }
        }
    }
    let active = media_db::active_storage_keys(pool).await?;
    stats.removed_orphans = storage.remove_orphans(&active).await?;
    Ok(stats)
}

pub async fn process_deletion_key(
    pool: &sqlx::MySqlPool,
    storage: &MediaStorage,
    storage_key: &str,
) -> Result<(), anyhow::Error> {
    match storage.remove(storage_key).await {
        Ok(()) => media_db::mark_deletion_processed_by_key(pool, storage_key).await?,
        Err(error) => {
            media_db::mark_deletion_failed_by_key(pool, storage_key, &error.to_string()).await?;
            return Err(error.into());
        }
    }
    Ok(())
}

pub fn process_photo(
    bytes: &[u8],
    config: &PhotoUploadConfig,
) -> Result<ProcessedPhoto, MediaError> {
    if bytes.is_empty() {
        return Err(MediaError::Invalid);
    }
    if bytes.len() > config.max_upload_bytes {
        return Err(MediaError::TooLarge);
    }

    let format = image::guess_format(bytes).map_err(|_| MediaError::Unsupported)?;
    validate_container(bytes, format)?;
    let mut image = decode_with_orientation(bytes, format, config)?;

    let (source_width, source_height) = image.dimensions();
    if source_width > config.max_edge || source_height > config.max_edge {
        image = image.resize(config.max_edge, config.max_edge, FilterType::Lanczos3);
    }

    let rgb = flatten_onto_white(&image);
    let (width, height) = rgb.dimensions();
    let mut output = Vec::new();
    JpegEncoder::new_with_quality(&mut output, config.jpeg_quality)
        .encode_image(&DynamicImage::ImageRgb8(rgb))
        .map_err(|_| MediaError::Invalid)?;

    // A successful second decode guards against returning a corrupt derivative.
    image::load_from_memory_with_format(&output, ImageFormat::Jpeg)
        .map_err(|_| MediaError::Invalid)?;
    Ok(ProcessedPhoto {
        bytes: output,
        media_type: "image/jpeg",
        width,
        height,
    })
}

fn decode_with_orientation(
    bytes: &[u8],
    format: ImageFormat,
    config: &PhotoUploadConfig,
) -> Result<DynamicImage, MediaError> {
    match format {
        ImageFormat::Jpeg => decode_decoder(JpegDecoder::new(Cursor::new(bytes)), config),
        ImageFormat::Png => decode_decoder(PngDecoder::new(Cursor::new(bytes)), config),
        ImageFormat::WebP => decode_decoder(WebPDecoder::new(Cursor::new(bytes)), config),
        _ => Err(MediaError::Unsupported),
    }
}

fn decode_decoder<D>(
    decoder: Result<D, image::ImageError>,
    config: &PhotoUploadConfig,
) -> Result<DynamicImage, MediaError>
where
    D: ImageDecoder,
{
    let mut decoder = decoder.map_err(|_| MediaError::Invalid)?;
    let (width, height) = decoder.dimensions();
    validate_dimensions(width, height, config)?;
    let orientation = decoder.orientation().map_err(|_| MediaError::Invalid)?;
    let mut image = DynamicImage::from_decoder(decoder).map_err(|_| MediaError::Invalid)?;
    image.apply_orientation(orientation);
    Ok(image)
}

fn validate_dimensions(
    width: u32,
    height: u32,
    config: &PhotoUploadConfig,
) -> Result<(), MediaError> {
    if width == 0
        || height == 0
        || width > config.max_source_dimension
        || height > config.max_source_dimension
        || u64::from(width)
            .checked_mul(u64::from(height))
            .is_none_or(|pixels| pixels > config.max_source_pixels)
    {
        return Err(MediaError::TooLarge);
    }
    Ok(())
}

fn flatten_onto_white(image: &DynamicImage) -> RgbImage {
    let rgba = image.to_rgba8();
    RgbImage::from_fn(rgba.width(), rgba.height(), |x, y| {
        let pixel = rgba.get_pixel(x, y).0;
        let alpha = u16::from(pixel[3]);
        let blend = |channel: u8| {
            let value = u16::from(channel) * alpha + 255 * (255 - alpha);
            #[allow(clippy::cast_possible_truncation)]
            let result = (value / 255) as u8;
            result
        };
        Rgb([blend(pixel[0]), blend(pixel[1]), blend(pixel[2])])
    })
}

fn validate_container(bytes: &[u8], format: ImageFormat) -> Result<(), MediaError> {
    match format {
        ImageFormat::Jpeg => {
            if bytes.len() < 4
                || !bytes.starts_with(&[0xff, 0xd8])
                || !bytes.ends_with(&[0xff, 0xd9])
            {
                return Err(MediaError::Invalid);
            }
        }
        ImageFormat::Png => validate_png_container(bytes)?,
        ImageFormat::WebP => {
            if bytes.len() < 12 || &bytes[..4] != b"RIFF" || &bytes[8..12] != b"WEBP" {
                return Err(MediaError::Invalid);
            }
            let declared =
                u32::from_le_bytes(bytes[4..8].try_into().map_err(|_| MediaError::Invalid)?);
            let total = usize::try_from(declared)
                .ok()
                .and_then(|size| size.checked_add(8))
                .ok_or(MediaError::Invalid)?;
            if total != bytes.len() {
                return Err(MediaError::Invalid);
            }
        }
        _ => return Err(MediaError::Unsupported),
    }
    Ok(())
}

fn validate_png_container(bytes: &[u8]) -> Result<(), MediaError> {
    const SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";
    if !bytes.starts_with(SIGNATURE) {
        return Err(MediaError::Invalid);
    }
    let mut offset = SIGNATURE.len();
    let mut saw_iend = false;
    while offset < bytes.len() {
        let header_end = offset.checked_add(8).ok_or(MediaError::Invalid)?;
        if header_end > bytes.len() {
            return Err(MediaError::Invalid);
        }
        let length = u32::from_be_bytes(
            bytes[offset..offset + 4]
                .try_into()
                .map_err(|_| MediaError::Invalid)?,
        );
        let chunk_type = &bytes[offset + 4..header_end];
        let chunk_end = header_end
            .checked_add(usize::try_from(length).map_err(|_| MediaError::Invalid)?)
            .and_then(|end| end.checked_add(4))
            .ok_or(MediaError::Invalid)?;
        if chunk_end > bytes.len() || saw_iend {
            return Err(MediaError::Invalid);
        }
        offset = chunk_end;
        if chunk_type == b"IEND" {
            if length != 0 {
                return Err(MediaError::Invalid);
            }
            saw_iend = true;
        }
    }
    if !saw_iend || offset != bytes.len() {
        return Err(MediaError::Invalid);
    }
    Ok(())
}

fn generate_storage_key() -> String {
    let mut bytes = [0_u8; 16];
    rand::rng().fill(&mut bytes);
    format!("{}.jpg", hex::encode(bytes))
}

fn is_valid_storage_key(key: &str) -> bool {
    let Some(hex_part) = key.strip_suffix(".jpg") else {
        return false;
    };
    hex_part.len() == 32 && hex_part.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use image::{ImageBuffer, Rgba};

    use super::*;

    fn encoded_test_image(format: ImageFormat, width: u32, height: u32) -> Vec<u8> {
        let image = DynamicImage::ImageRgba8(ImageBuffer::from_pixel(
            width,
            height,
            Rgba([20, 40, 60, 128]),
        ));
        let mut bytes = Cursor::new(Vec::new());
        image.write_to(&mut bytes, format).unwrap();
        bytes.into_inner()
    }

    fn with_exif_orientation(jpeg: &[u8], orientation: u16) -> Vec<u8> {
        assert!(jpeg.starts_with(&[0xff, 0xd8]));
        let mut tiff = Vec::new();
        tiff.extend_from_slice(b"II");
        tiff.extend_from_slice(&42_u16.to_le_bytes());
        tiff.extend_from_slice(&8_u32.to_le_bytes());
        tiff.extend_from_slice(&1_u16.to_le_bytes());
        tiff.extend_from_slice(&0x0112_u16.to_le_bytes());
        tiff.extend_from_slice(&3_u16.to_le_bytes());
        tiff.extend_from_slice(&1_u32.to_le_bytes());
        tiff.extend_from_slice(&orientation.to_le_bytes());
        tiff.extend_from_slice(&0_u16.to_le_bytes());
        tiff.extend_from_slice(&0_u32.to_le_bytes());
        let mut payload = b"Exif\0\0".to_vec();
        payload.extend_from_slice(&tiff);
        let segment_length = u16::try_from(payload.len() + 2).unwrap();

        let mut result = jpeg[..2].to_vec();
        result.extend_from_slice(&[0xff, 0xe1]);
        result.extend_from_slice(&segment_length.to_be_bytes());
        result.extend_from_slice(&payload);
        result.extend_from_slice(&jpeg[2..]);
        result
    }

    #[test]
    fn accepts_and_normalizes_all_supported_formats() {
        let config = PhotoUploadConfig::default();
        for format in [ImageFormat::Jpeg, ImageFormat::Png, ImageFormat::WebP] {
            let processed = process_photo(&encoded_test_image(format, 20, 10), &config).unwrap();
            assert_eq!(processed.media_type, "image/jpeg");
            assert_eq!((processed.width, processed.height), (20, 10));
            assert_eq!(
                image::guess_format(&processed.bytes).unwrap(),
                ImageFormat::Jpeg
            );
        }
    }

    #[test]
    fn resizes_without_upscaling_and_preserves_aspect_ratio() {
        let config = PhotoUploadConfig {
            max_edge: 100,
            ..PhotoUploadConfig::default()
        };
        let small = process_photo(&encoded_test_image(ImageFormat::Png, 20, 10), &config).unwrap();
        assert_eq!((small.width, small.height), (20, 10));
        let large =
            process_photo(&encoded_test_image(ImageFormat::Png, 200, 100), &config).unwrap();
        assert_eq!((large.width, large.height), (100, 50));
    }

    #[test]
    fn transparent_pixels_are_composited_onto_white() {
        let processed = process_photo(
            &encoded_test_image(ImageFormat::Png, 1, 1),
            &PhotoUploadConfig::default(),
        )
        .unwrap();
        let pixel = image::load_from_memory(&processed.bytes)
            .unwrap()
            .to_rgb8()
            .get_pixel(0, 0)
            .0;
        assert!(pixel[0] > 100);
        assert!(pixel[1] > 100);
        assert!(pixel[2] > 100);
    }

    #[test]
    fn applies_exif_orientation_and_removes_metadata() {
        let jpeg = encoded_test_image(ImageFormat::Jpeg, 2, 1);
        let oriented = with_exif_orientation(&jpeg, 6);
        assert!(oriented.windows(6).any(|window| window == b"Exif\0\0"));

        let processed = process_photo(&oriented, &PhotoUploadConfig::default()).unwrap();

        assert_eq!((processed.width, processed.height), (1, 2));
        assert!(
            !processed
                .bytes
                .windows(6)
                .any(|window| window == b"Exif\0\0")
        );
    }

    #[test]
    fn rejects_unsupported_malformed_oversized_and_trailing_payloads() {
        let config = PhotoUploadConfig {
            max_upload_bytes: 100,
            ..PhotoUploadConfig::default()
        };
        for unsupported in [
            b"GIF89a".as_slice(),
            b"<svg xmlns='http://www.w3.org/2000/svg'/>".as_slice(),
            b"PK\x03\x04zip".as_slice(),
            b"MZ executable".as_slice(),
            b"\0\0\0\x18ftypheic".as_slice(),
        ] {
            assert!(matches!(
                process_photo(unsupported, &config),
                Err(MediaError::Unsupported)
            ));
        }
        assert!(matches!(
            process_photo(b"\x89PNG\r\n\x1a\ntruncated", &config),
            Err(MediaError::Invalid)
        ));
        assert!(matches!(
            process_photo(&[0; 101], &config),
            Err(MediaError::TooLarge)
        ));

        let mut png = encoded_test_image(ImageFormat::Png, 1, 1);
        png.extend_from_slice(b"executable payload");
        assert!(matches!(
            process_photo(&png, &PhotoUploadConfig::default()),
            Err(MediaError::Invalid)
        ));
    }

    #[test]
    fn upload_byte_limit_is_inclusive() {
        let png = encoded_test_image(ImageFormat::Png, 1, 1);
        let accepted = PhotoUploadConfig {
            max_upload_bytes: png.len(),
            ..PhotoUploadConfig::default()
        };
        assert!(process_photo(&png, &accepted).is_ok());

        let rejected = PhotoUploadConfig {
            max_upload_bytes: png.len() - 1,
            ..PhotoUploadConfig::default()
        };
        assert!(matches!(
            process_photo(&png, &rejected),
            Err(MediaError::TooLarge)
        ));
    }

    #[test]
    fn rejects_dimensions_and_pixel_counts_before_decode() {
        let bytes = encoded_test_image(ImageFormat::Png, 20, 20);
        let config = PhotoUploadConfig {
            max_source_dimension: 10,
            max_source_pixels: 100,
            max_edge: 10,
            ..PhotoUploadConfig::default()
        };
        assert!(matches!(
            process_photo(&bytes, &config),
            Err(MediaError::TooLarge)
        ));
    }

    #[test]
    fn storage_keys_are_random_safe_and_pathless() {
        let first = generate_storage_key();
        let second = generate_storage_key();
        assert_ne!(first, second);
        assert!(is_valid_storage_key(&first));
        assert!(!is_valid_storage_key("../secret.jpg"));
        assert!(!is_valid_storage_key("not-hex.jpg"));
        assert!(!is_valid_storage_key(
            "0123456789abcdef0123456789abcdef.svg"
        ));
    }

    #[tokio::test]
    async fn storage_stages_commits_reads_and_removes_idempotently() {
        let suffix = generate_storage_key();
        let root = std::env::temp_dir().join(format!("lilly-media-test-{suffix}"));
        let storage = MediaStorage::new(&root);
        let staged = storage.stage(b"test bytes").await.unwrap();
        assert!(staged.temporary_path.exists());
        storage.commit(&staged).await.unwrap();
        assert_eq!(
            storage.read(&staged.storage_key).await.unwrap(),
            b"test bytes"
        );
        storage.remove(&staged.storage_key).await.unwrap();
        storage.remove(&staged.storage_key).await.unwrap();
        let _ = tokio::fs::remove_dir_all(root).await;
    }

    #[tokio::test]
    async fn discarding_a_staged_upload_removes_only_the_temporary_file() {
        let suffix = generate_storage_key();
        let root = std::env::temp_dir().join(format!("lilly-media-discard-test-{suffix}"));
        let storage = MediaStorage::new(&root);
        let staged = storage.stage(b"test bytes").await.unwrap();

        storage.discard_staged(&staged).await;

        assert!(!staged.temporary_path.exists());
        assert!(!staged.final_path.exists());
        let _ = tokio::fs::remove_dir_all(root).await;
    }
}
