use std::io::Cursor;

use image::{ImageFormat, ImageReader, Limits};
use reqwest::Client;

use lilly_importer_core::{AdapterError, CoverData};

pub(crate) const MAX_COVER_IMAGE_BYTES: usize = 5 * 1024 * 1024;
const MAX_COVER_IMAGE_DIMENSION: u32 = 10_000;
const MAX_DECODED_COVER_IMAGE_BYTES: u64 = 128 * 1024 * 1024;

pub(crate) async fn download_cover_image(
    client: &Client,
    url: &str,
) -> Result<CoverData, AdapterError> {
    let mut response = client.get(url).send().await?.error_for_status()?;
    if response
        .content_length()
        .is_some_and(|length| length > MAX_COVER_IMAGE_BYTES as u64)
    {
        return Err(image_too_large_error());
    }

    let capacity = response
        .content_length()
        .and_then(|length| usize::try_from(length).ok())
        .unwrap_or_default();
    let mut bytes = Vec::with_capacity(capacity);
    while let Some(chunk) = response.chunk().await? {
        append_limited(&mut bytes, &chunk)?;
    }

    sanitize_cover_image(&bytes)
}

fn append_limited(buffer: &mut Vec<u8>, chunk: &[u8]) -> Result<(), AdapterError> {
    if buffer
        .len()
        .checked_add(chunk.len())
        .is_none_or(|length| length > MAX_COVER_IMAGE_BYTES)
    {
        return Err(image_too_large_error());
    }
    buffer.extend_from_slice(chunk);
    Ok(())
}

fn sanitize_cover_image(bytes: &[u8]) -> Result<CoverData, AdapterError> {
    if bytes.len() > MAX_COVER_IMAGE_BYTES {
        return Err(image_too_large_error());
    }

    let format = image::guess_format(bytes)
        .map_err(|error| invalid_image_error(&format!("unknown format: {error}")))?;
    let content_type = match format {
        ImageFormat::Jpeg => "image/jpeg",
        ImageFormat::Png => "image/png",
        ImageFormat::WebP => "image/webp",
        _ => {
            return Err(invalid_image_error(&format!(
                "unsupported format {format:?}"
            )));
        }
    };

    let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_COVER_IMAGE_DIMENSION);
    limits.max_image_height = Some(MAX_COVER_IMAGE_DIMENSION);
    limits.max_alloc = Some(MAX_DECODED_COVER_IMAGE_BYTES);
    reader.limits(limits);
    let decoded = reader
        .decode()
        .map_err(|error| invalid_image_error(&format!("decoding failed: {error}")))?;

    let mut sanitized = Cursor::new(Vec::new());
    decoded
        .write_to(&mut sanitized, format)
        .map_err(|error| invalid_image_error(&format!("re-encoding failed: {error}")))?;
    let sanitized = sanitized.into_inner();
    if sanitized.len() > MAX_COVER_IMAGE_BYTES {
        return Err(image_too_large_error());
    }

    Ok(CoverData {
        bytes: sanitized,
        content_type: content_type.to_string(),
    })
}

fn image_too_large_error() -> AdapterError {
    AdapterError::Parse(format!(
        "Cover image exceeds the {MAX_COVER_IMAGE_BYTES} byte limit"
    ))
}

fn invalid_image_error(message: &str) -> AdapterError {
    AdapterError::Parse(format!("Invalid cover image: {message}"))
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use image::{DynamicImage, ImageBuffer, ImageFormat, Rgba};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;
    use tokio::task::JoinHandle;

    use super::*;

    fn encoded_test_image(format: ImageFormat) -> Vec<u8> {
        let image =
            DynamicImage::ImageRgba8(ImageBuffer::from_pixel(2, 2, Rgba([20, 40, 60, 255])));
        let mut bytes = Cursor::new(Vec::new());
        image.write_to(&mut bytes, format).unwrap();
        bytes.into_inner()
    }

    async fn serve_once(headers: &str, body: Vec<u8>) -> (String, JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let response_head = format!("HTTP/1.1 200 OK\r\nConnection: close\r\n{headers}\r\n");
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = [0_u8; 1024];
            let _ = stream.read(&mut request).await;
            stream.write_all(response_head.as_bytes()).await.unwrap();
            let _ = stream.write_all(&body).await;
        });
        (format!("http://{address}/cover"), server)
    }

    #[test]
    fn sanitizes_and_reencodes_supported_formats() {
        for (format, expected_content_type) in [
            (ImageFormat::Jpeg, "image/jpeg"),
            (ImageFormat::Png, "image/png"),
            (ImageFormat::WebP, "image/webp"),
        ] {
            let original = encoded_test_image(format);
            let cover = sanitize_cover_image(&original).unwrap();

            assert_eq!(cover.content_type, expected_content_type);
            assert_eq!(image::guess_format(&cover.bytes).unwrap(), format);
            image::load_from_memory_with_format(&cover.bytes, format).unwrap();
        }
    }

    #[test]
    fn reencoding_removes_trailing_payload() {
        let mut original = encoded_test_image(ImageFormat::Png);
        let trailing_payload = b"untrusted trailing payload";
        original.extend_from_slice(trailing_payload);

        let cover = sanitize_cover_image(&original).unwrap();

        assert!(!cover.bytes.ends_with(trailing_payload));
        assert!(cover.bytes.len() < original.len());
    }

    #[test]
    fn rejects_unsupported_and_malformed_images() {
        let gif = b"GIF89a\x01\x00\x01\x00";
        assert!(matches!(
            sanitize_cover_image(gif),
            Err(AdapterError::Parse(message)) if message.contains("unsupported format Gif")
        ));
        assert!(matches!(
            sanitize_cover_image(b"not an image"),
            Err(AdapterError::Parse(message)) if message.contains("unknown format")
        ));
        assert!(matches!(
            sanitize_cover_image(b"\x89PNG\r\n\x1a\ntruncated"),
            Err(AdapterError::Parse(message)) if message.contains("decoding failed")
        ));
    }

    #[test]
    fn streaming_buffer_enforces_five_megabyte_limit() {
        let mut buffer = Vec::new();
        append_limited(&mut buffer, &vec![0; MAX_COVER_IMAGE_BYTES]).unwrap();
        assert_eq!(buffer.len(), MAX_COVER_IMAGE_BYTES);
        assert!(matches!(
            append_limited(&mut buffer, &[0]),
            Err(AdapterError::Parse(message)) if message.contains("exceeds")
        ));
        assert!(matches!(
            sanitize_cover_image(&vec![0; MAX_COVER_IMAGE_BYTES + 1]),
            Err(AdapterError::Parse(message)) if message.contains("exceeds")
        ));
    }

    #[tokio::test]
    async fn download_uses_detected_format_instead_of_response_header() {
        let png = encoded_test_image(ImageFormat::Png);
        let headers = format!(
            "Content-Type: text/html\r\nContent-Length: {}\r\n",
            png.len()
        );
        let (url, server) = serve_once(&headers, png).await;

        let cover = download_cover_image(&Client::new(), &url).await.unwrap();
        server.await.unwrap();

        assert_eq!(cover.content_type, "image/png");
        assert_eq!(image::guess_format(&cover.bytes).unwrap(), ImageFormat::Png);
    }

    #[tokio::test]
    async fn download_rejects_oversized_content_length_before_buffering() {
        let headers = format!("Content-Length: {}\r\n", MAX_COVER_IMAGE_BYTES + 1);
        let (url, server) = serve_once(&headers, Vec::new()).await;

        let result = download_cover_image(&Client::new(), &url).await;
        server.await.unwrap();

        assert!(matches!(
            result,
            Err(AdapterError::Parse(message)) if message.contains("exceeds")
        ));
    }

    #[tokio::test]
    async fn download_rejects_oversized_stream_without_content_length() {
        let (url, server) = serve_once("", vec![0; MAX_COVER_IMAGE_BYTES + 1]).await;

        let result = download_cover_image(&Client::new(), &url).await;
        server.await.unwrap();

        assert!(matches!(
            result,
            Err(AdapterError::Parse(message)) if message.contains("exceeds")
        ));
    }
}
