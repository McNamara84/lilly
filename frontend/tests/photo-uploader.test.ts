import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import PhotoUploader from '$lib/components/media/PhotoUploader.svelte';

const mocks = vi.hoisted(() => ({
	fetchPhotoPolicy: vi.fn(),
	fetchCollectionPhotos: vi.fn(),
	uploadCollectionPhoto: vi.fn(),
	deleteCollectionPhoto: vi.fn()
}));

const objectUrlMocks = vi.hoisted(() => ({
	create: vi.fn(() => 'blob:photo-preview'),
	revoke: vi.fn()
}));

vi.mock('$lib/api/media', async (importOriginal) => {
	const original = await importOriginal<typeof import('$lib/api/media')>();
	return {
		...original,
		fetchPhotoPolicy: mocks.fetchPhotoPolicy,
		fetchCollectionPhotos: mocks.fetchCollectionPhotos,
		uploadCollectionPhoto: mocks.uploadCollectionPhoto,
		deleteCollectionPhoto: mocks.deleteCollectionPhoto
	};
});

const policy = {
	allowed_media_types: ['image/jpeg', 'image/png', 'image/webp'],
	max_upload_bytes: 5 * 1024 * 1024,
	max_photos: 4,
	max_edge: 2048
};

const photos = Array.from({ length: 4 }, (_, index) => ({
	id: index + 1,
	content_url: `/api/v1/collection-photos/${index + 1}/content`,
	sort_order: index,
	media_type: 'image/jpeg',
	byte_size: 100,
	width: 20,
	height: 10,
	created_at: '2026-08-11T08:00:00'
}));

describe('PhotoUploader', () => {
	beforeEach(() => {
		vi.clearAllMocks();
		Object.defineProperties(URL, {
			createObjectURL: { configurable: true, value: objectUrlMocks.create },
			revokeObjectURL: { configurable: true, value: objectUrlMocks.revoke }
		});
		mocks.fetchPhotoPolicy.mockResolvedValue(policy);
		mocks.fetchCollectionPhotos.mockResolvedValue([]);
		mocks.uploadCollectionPhoto.mockImplementation(
			(_entryId: number, _file: File, onProgress: (value: number) => void) => {
				onProgress(60);
				return Promise.resolve(photos[0]);
			}
		);
		mocks.deleteCollectionPhoto.mockResolvedValue(undefined);
		vi.spyOn(window, 'confirm').mockReturnValue(true);
	});

	it('loads policy and photos and exposes camera-compatible input', async () => {
		render(PhotoUploader, { props: { entryId: 17 } });
		const input = (await screen.findByTestId('photo-file-input')) as HTMLInputElement;

		expect(mocks.fetchPhotoPolicy).toHaveBeenCalledOnce();
		expect(mocks.fetchCollectionPhotos).toHaveBeenCalledWith(17, expect.any(AbortSignal));
		expect(input).toHaveAttribute('accept', 'image/jpeg,image/png,image/webp');
		expect(input).toHaveAttribute('capture', 'environment');
		expect(input).toHaveAttribute('multiple');
		expect(screen.getByText(/JPEG, PNG oder WebP bis 5 MiB/)).toBeInTheDocument();
	});

	it('uploads a valid file and adds the returned preview', async () => {
		render(PhotoUploader, { props: { entryId: 17 } });
		const input = (await screen.findByTestId('photo-file-input')) as HTMLInputElement;
		const file = new File(['valid'], 'condition.png', { type: 'image/png' });

		await userEvent.upload(input, file);
		await waitFor(() => expect(mocks.uploadCollectionPhoto).toHaveBeenCalledOnce());
		expect(mocks.uploadCollectionPhoto).toHaveBeenCalledWith(
			17,
			file,
			expect.any(Function),
			expect.any(AbortSignal)
		);
		await waitFor(() => expect(screen.getByTestId('photo-count')).toHaveTextContent('1/4'));
		expect(screen.getByAltText('Eigenes Foto 1 des Sammlungsexemplars')).toHaveAttribute(
			'src',
			photos[0].content_url
		);
		expect(screen.getByRole('status')).toHaveTextContent('wurde hochgeladen');
		expect(objectUrlMocks.create).toHaveBeenCalledWith(file);
		expect(objectUrlMocks.revoke).toHaveBeenCalledWith('blob:photo-preview');
	});

	it('aborts an active upload and releases its preview when unmounted', async () => {
		mocks.uploadCollectionPhoto.mockImplementation(
			(_entryId: number, _file: File, _progress: (value: number) => void, signal?: AbortSignal) =>
				new Promise((_resolve, reject) => {
					signal?.addEventListener('abort', () =>
						reject(new DOMException('Foto-Upload abgebrochen', 'AbortError'))
					);
				})
		);
		const view = render(PhotoUploader, { props: { entryId: 17 } });
		await userEvent.upload(
			await screen.findByTestId('photo-file-input'),
			new File(['valid'], 'condition.png', { type: 'image/png' })
		);
		await screen.findByTestId('pending-photo-preview');

		view.unmount();

		await waitFor(() => expect(objectUrlMocks.revoke).toHaveBeenCalledWith('blob:photo-preview'));
	});

	it('shows a local preview and progress while an upload is pending', async () => {
		let finishUpload: ((photo: (typeof photos)[number]) => void) | undefined;
		mocks.uploadCollectionPhoto.mockImplementation(
			(_entryId: number, _file: File, onProgress: (value: number) => void) => {
				onProgress(35);
				return new Promise((resolve) => {
					finishUpload = resolve;
				});
			}
		);
		render(PhotoUploader, { props: { entryId: 17 } });
		const file = new File(['valid'], 'condition.png', { type: 'image/png' });

		await userEvent.upload(await screen.findByTestId('photo-file-input'), file);
		const preview = await screen.findByTestId('pending-photo-preview');
		expect(screen.getByAltText('Lokale Vorschau von condition.png')).toHaveAttribute(
			'src',
			'blob:photo-preview'
		);
		expect(preview).toHaveTextContent('Upload 35%');
		expect(screen.getByRole('progressbar')).toHaveAttribute('aria-valuenow', '35');

		finishUpload?.(photos[0]);
		await waitFor(() =>
			expect(screen.queryByTestId('pending-photo-preview')).not.toBeInTheDocument()
		);
		expect(objectUrlMocks.revoke).toHaveBeenCalledWith('blob:photo-preview');
	});

	it('rejects unsupported and oversized files before upload', async () => {
		render(PhotoUploader, { props: { entryId: 17 } });
		const input = (await screen.findByTestId('photo-file-input')) as HTMLInputElement;

		await userEvent.upload(input, new File(['svg'], 'unsafe.svg', { type: 'image/svg+xml' }), {
			applyAccept: false
		});
		expect(await screen.findByRole('alert')).toHaveTextContent('kein unterstütztes');
		const oversized = new File([new Uint8Array(policy.max_upload_bytes + 1)], 'huge.png', {
			type: 'image/png'
		});
		await userEvent.upload(input, oversized);
		expect(await screen.findByRole('alert')).toHaveTextContent('größer als 5 MiB');
		expect(mocks.uploadCollectionPhoto).not.toHaveBeenCalled();
	});

	it('enforces four slots and frees a slot after confirmed deletion', async () => {
		mocks.fetchCollectionPhotos.mockResolvedValue(photos);
		render(PhotoUploader, { props: { entryId: 17 } });

		await waitFor(() => expect(screen.getByTestId('photo-count')).toHaveTextContent('4/4'));
		expect(screen.getByTestId('photo-dropzone')).toBeDisabled();
		expect(screen.getByText('Alle vier Foto-Slots sind belegt')).toBeInTheDocument();

		await userEvent.click(screen.getByTestId('delete-photo-1'));
		expect(window.confirm).toHaveBeenCalledWith('Dieses Foto wirklich löschen?');
		expect(mocks.deleteCollectionPhoto).toHaveBeenCalledWith(17, 1);
		await waitFor(() => expect(screen.getByTestId('photo-count')).toHaveTextContent('3/4'));
		expect(screen.getByTestId('photo-dropzone')).not.toBeDisabled();
	});

	it('supports drag and drop and reports backend failures without losing existing photos', async () => {
		mocks.fetchCollectionPhotos.mockResolvedValue([photos[0]]);
		mocks.uploadCollectionPhoto.mockRejectedValue(new Error('Invalid or malformed image'));
		render(PhotoUploader, { props: { entryId: 17 } });
		const dropzone = await screen.findByTestId('photo-dropzone');
		const file = new File(['invalid'], 'fake.png', { type: 'image/png' });

		await fireEvent.drop(dropzone, { dataTransfer: { files: [file] } });
		expect(await screen.findByRole('alert')).toHaveTextContent('Invalid or malformed image');
		expect(screen.getByTestId('photo-count')).toHaveTextContent('1/4');
		expect(screen.getByAltText('Eigenes Foto 1 des Sammlungsexemplars')).toBeInTheDocument();
	});

	it('falls back to the default policy and reports photo-list loading failures', async () => {
		mocks.fetchPhotoPolicy.mockRejectedValue(new Error('policy offline'));
		mocks.fetchCollectionPhotos.mockRejectedValue('untyped failure');
		render(PhotoUploader, { props: { entryId: 17 } });

		expect(await screen.findByRole('alert')).toHaveTextContent(
			'Fotos konnten nicht geladen werden.'
		);
		expect(mocks.fetchPhotoPolicy).toHaveBeenCalledOnce();
		expect(mocks.fetchCollectionPhotos).toHaveBeenCalledOnce();
	});

	it('opens and closes the enlarged photo dialog', async () => {
		mocks.fetchCollectionPhotos.mockResolvedValue([photos[0]]);
		render(PhotoUploader, { props: { entryId: 17 } });

		await userEvent.click(await screen.findByRole('button', { name: 'Foto 1 vergrößern' }));
		expect(screen.getByRole('dialog', { name: 'Fotoansicht' })).toBeInTheDocument();
		expect(screen.getByAltText('Vergrößertes eigenes Foto des Sammlungsexemplars')).toHaveAttribute(
			'src',
			photos[0].content_url
		);
		await userEvent.click(
			screen.getByRole('button', { name: 'Vergrößerte Fotoansicht schließen' })
		);
		expect(screen.queryByRole('dialog', { name: 'Fotoansicht' })).not.toBeInTheDocument();
	});

	it('preserves a photo when deletion is cancelled or fails', async () => {
		mocks.fetchCollectionPhotos.mockResolvedValue([photos[0]]);
		vi.mocked(window.confirm).mockReturnValueOnce(false).mockReturnValueOnce(true);
		mocks.deleteCollectionPhoto.mockRejectedValueOnce('untyped failure');
		render(PhotoUploader, { props: { entryId: 17 } });
		const deleteButton = await screen.findByTestId('delete-photo-1');

		await userEvent.click(deleteButton);
		expect(mocks.deleteCollectionPhoto).not.toHaveBeenCalled();
		await userEvent.click(deleteButton);
		expect(await screen.findByRole('alert')).toHaveTextContent(
			'Foto konnte nicht gelöscht werden.'
		);
		expect(screen.getByTestId('photo-count')).toHaveTextContent('1/4');
	});

	it('uploads only as many dropped files as there are free slots', async () => {
		mocks.fetchCollectionPhotos.mockResolvedValue(photos.slice(0, 3));
		mocks.uploadCollectionPhoto.mockResolvedValue({ ...photos[3], id: 40 });
		render(PhotoUploader, { props: { entryId: 17 } });
		const dropzone = await screen.findByTestId('photo-dropzone');
		const first = new File(['one'], 'one.png', { type: 'image/png' });
		const second = new File(['two'], 'two.png', { type: 'image/png' });

		await fireEvent.drop(dropzone, { dataTransfer: { files: [first, second] } });
		await waitFor(() => expect(mocks.uploadCollectionPhoto).toHaveBeenCalledOnce());
		expect(mocks.uploadCollectionPhoto).toHaveBeenCalledWith(
			17,
			first,
			expect.any(Function),
			expect.any(AbortSignal)
		);
		expect(screen.getByTestId('photo-count')).toHaveTextContent('4/4');
	});
});
