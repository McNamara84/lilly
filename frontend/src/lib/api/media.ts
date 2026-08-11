const API_BASE = '/api/v1';

export interface CollectionPhoto {
	id: number;
	content_url: string;
	sort_order: number;
	media_type: string;
	byte_size: number;
	width: number;
	height: number;
	created_at: string;
}

export interface PhotoPolicy {
	allowed_media_types: string[];
	max_upload_bytes: number;
	max_photos: number;
	max_edge: number;
}

export const DEFAULT_PHOTO_POLICY: PhotoPolicy = {
	allowed_media_types: ['image/jpeg', 'image/png', 'image/webp'],
	max_upload_bytes: 5 * 1024 * 1024,
	max_photos: 4,
	max_edge: 2048
};

async function errorMessage(response: Response): Promise<string> {
	const body = await response.json().catch(() => null);
	return typeof body?.error === 'string' && body.error
		? body.error
		: `Foto-Anfrage fehlgeschlagen (${response.status})`;
}

export async function fetchPhotoPolicy(signal?: AbortSignal): Promise<PhotoPolicy> {
	const response = await fetch(`${API_BASE}/media/photo-policy`, { signal });
	if (!response.ok) throw new Error(await errorMessage(response));
	return response.json();
}

export async function fetchCollectionPhotos(
	entryId: number,
	signal?: AbortSignal
): Promise<CollectionPhoto[]> {
	const response = await fetch(`${API_BASE}/me/collection/${entryId}/photos`, {
		credentials: 'same-origin',
		signal
	});
	if (!response.ok) throw new Error(await errorMessage(response));
	return response.json();
}

export function uploadCollectionPhoto(
	entryId: number,
	file: File,
	onProgress: (percent: number) => void,
	signal?: AbortSignal
): Promise<CollectionPhoto> {
	return new Promise((resolve, reject) => {
		const request = new XMLHttpRequest();
		const abort = () => request.abort();
		request.open('POST', `${API_BASE}/me/collection/${entryId}/photos`);
		request.responseType = 'json';
		request.withCredentials = true;
		request.upload.addEventListener('progress', (event) => {
			if (event.lengthComputable && event.total > 0) {
				onProgress(Math.min(100, Math.round((event.loaded / event.total) * 100)));
			}
		});
		request.addEventListener('load', () => {
			signal?.removeEventListener('abort', abort);
			if (request.status >= 200 && request.status < 300) {
				onProgress(100);
				resolve(request.response as CollectionPhoto);
				return;
			}
			const message =
				typeof request.response?.error === 'string' && request.response.error
					? request.response.error
					: `Foto-Upload fehlgeschlagen (${request.status})`;
			reject(new Error(message));
		});
		request.addEventListener('error', () => {
			signal?.removeEventListener('abort', abort);
			reject(new Error('Die Verbindung beim Foto-Upload ist abgebrochen.'));
		});
		request.addEventListener('abort', () => {
			signal?.removeEventListener('abort', abort);
			reject(new DOMException('Foto-Upload abgebrochen', 'AbortError'));
		});
		if (signal?.aborted) {
			request.abort();
			return;
		}
		signal?.addEventListener('abort', abort, { once: true });
		const form = new FormData();
		form.append('photo', file, file.name);
		request.send(form);
	});
}

export async function deleteCollectionPhoto(
	entryId: number,
	photoId: number,
	signal?: AbortSignal
): Promise<void> {
	const response = await fetch(`${API_BASE}/me/collection/${entryId}/photos/${photoId}`, {
		method: 'DELETE',
		credentials: 'same-origin',
		signal
	});
	if (!response.ok) throw new Error(await errorMessage(response));
}
