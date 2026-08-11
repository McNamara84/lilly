import { beforeEach, describe, expect, it, vi } from 'vitest';

import {
	deleteCollectionPhoto,
	fetchCollectionPhotos,
	fetchPhotoPolicy,
	uploadCollectionPhoto
} from '$lib/api/media';

const fetchMock = vi.fn();
vi.stubGlobal('fetch', fetchMock);

describe('media API', () => {
	beforeEach(() => {
		vi.clearAllMocks();
	});

	it('loads the public policy and owner photo list', async () => {
		fetchMock
			.mockResolvedValueOnce({
				ok: true,
				json: async () => ({
					allowed_media_types: ['image/jpeg', 'image/png', 'image/webp'],
					max_upload_bytes: 5_242_880,
					max_photos: 4,
					max_edge: 2048
				})
			})
			.mockResolvedValueOnce({ ok: true, json: async () => [{ id: 4 }] });

		await expect(fetchPhotoPolicy()).resolves.toMatchObject({ max_photos: 4 });
		await expect(fetchCollectionPhotos(9)).resolves.toEqual([{ id: 4 }]);
		expect(fetchMock).toHaveBeenNthCalledWith(1, '/api/v1/media/photo-policy', {
			signal: undefined
		});
		expect(fetchMock).toHaveBeenNthCalledWith(2, '/api/v1/me/collection/9/photos', {
			credentials: 'same-origin',
			signal: undefined
		});
	});

	it('surfaces JSON API errors and deletes with owner credentials', async () => {
		fetchMock
			.mockResolvedValueOnce({
				ok: false,
				status: 404,
				json: async () => ({ error: 'Photo not found' })
			})
			.mockResolvedValueOnce({ ok: true });

		await expect(fetchCollectionPhotos(12)).rejects.toThrow('Photo not found');
		await expect(deleteCollectionPhoto(12, 3)).resolves.toBeUndefined();
		expect(fetchMock).toHaveBeenLastCalledWith('/api/v1/me/collection/12/photos/3', {
			method: 'DELETE',
			credentials: 'same-origin',
			signal: undefined
		});
	});

	it('uses stable fallback messages for malformed error responses', async () => {
		fetchMock
			.mockResolvedValueOnce({
				ok: false,
				status: 503,
				json: async () => {
					throw new SyntaxError('not JSON');
				}
			})
			.mockResolvedValueOnce({
				ok: false,
				status: 500,
				json: async () => ({ error: '' })
			});

		await expect(fetchPhotoPolicy()).rejects.toThrow('Foto-Anfrage fehlgeschlagen (503)');
		await expect(deleteCollectionPhoto(12, 3)).rejects.toThrow('Foto-Anfrage fehlgeschlagen (500)');
	});

	it('uploads multipart data and reports real XHR progress', async () => {
		const requests: FakeXmlHttpRequest[] = [];
		vi.stubGlobal(
			'XMLHttpRequest',
			class extends FakeXmlHttpRequest {
				constructor() {
					super();
					requests.push(this);
				}
			}
		);
		const progress = vi.fn();
		const file = new File(['photo'], 'condition.png', { type: 'image/png' });

		const promise = uploadCollectionPhoto(7, file, progress);
		const request = requests[0];
		expect(request.method).toBe('POST');
		expect(request.url).toBe('/api/v1/me/collection/7/photos');
		expect(request.withCredentials).toBe(true);
		expect(request.sentBody).toBeInstanceOf(FormData);
		request.emitUpload('progress', { lengthComputable: true, loaded: 3, total: 4 });
		request.emitUpload('progress', { lengthComputable: true, loaded: 7, total: 4 });
		request.emitUpload('progress', { lengthComputable: false, loaded: 4, total: 4 });
		request.status = 201;
		request.response = { id: 44, content_url: '/photo/44' };
		request.emit('load');

		await expect(promise).resolves.toMatchObject({ id: 44 });
		expect(progress).toHaveBeenNthCalledWith(1, 75);
		expect(progress).toHaveBeenNthCalledWith(2, 100);
		expect(progress).toHaveBeenLastCalledWith(100);
	});

	it('rejects a failed XHR with the backend message', async () => {
		const requests: FakeXmlHttpRequest[] = [];
		vi.stubGlobal(
			'XMLHttpRequest',
			class extends FakeXmlHttpRequest {
				constructor() {
					super();
					requests.push(this);
				}
			}
		);
		const promise = uploadCollectionPhoto(
			7,
			new File(['bad'], 'bad.svg', { type: 'image/svg+xml' }),
			vi.fn()
		);
		requests[0].status = 400;
		requests[0].response = { error: 'Invalid or malformed image' };
		requests[0].emit('load');

		await expect(promise).rejects.toThrow('Invalid or malformed image');
	});

	it('uses a stable fallback when a failed XHR has no JSON error', async () => {
		const requests = captureRequests();
		const promise = uploadCollectionPhoto(
			7,
			new File(['bad'], 'bad.png', { type: 'image/png' }),
			vi.fn()
		);
		requests[0].status = 502;
		requests[0].emit('load');

		await expect(promise).rejects.toThrow('Foto-Upload fehlgeschlagen (502)');
	});

	it('reports transport errors and supports cancellation before and during upload', async () => {
		let requests = captureRequests();
		let promise = uploadCollectionPhoto(
			7,
			new File(['photo'], 'condition.png', { type: 'image/png' }),
			vi.fn()
		);
		requests[0].emit('error');
		await expect(promise).rejects.toThrow('Die Verbindung beim Foto-Upload ist abgebrochen.');

		const alreadyAborted = new AbortController();
		alreadyAborted.abort();
		requests = captureRequests();
		promise = uploadCollectionPhoto(
			7,
			new File(['photo'], 'condition.png', { type: 'image/png' }),
			vi.fn(),
			alreadyAborted.signal
		);
		await expect(promise).rejects.toMatchObject({ name: 'AbortError' });
		expect(requests[0].sentBody).toBeNull();

		const controller = new AbortController();
		requests = captureRequests();
		promise = uploadCollectionPhoto(
			7,
			new File(['photo'], 'condition.png', { type: 'image/png' }),
			vi.fn(),
			controller.signal
		);
		controller.abort();
		await expect(promise).rejects.toMatchObject({ name: 'AbortError' });
		expect(requests[0].sentBody).toBeInstanceOf(FormData);
	});
});

function captureRequests(): FakeXmlHttpRequest[] {
	const requests: FakeXmlHttpRequest[] = [];
	vi.stubGlobal(
		'XMLHttpRequest',
		class extends FakeXmlHttpRequest {
			constructor() {
				super();
				requests.push(this);
			}
		}
	);
	return requests;
}

type Listener = (event?: unknown) => void;

class FakeEventTarget {
	private listeners = new Map<string, Listener[]>();

	addEventListener(name: string, listener: Listener) {
		this.listeners.set(name, [...(this.listeners.get(name) ?? []), listener]);
	}

	removeEventListener(name: string, listener: Listener) {
		this.listeners.set(
			name,
			(this.listeners.get(name) ?? []).filter((candidate) => candidate !== listener)
		);
	}

	emit(name: string, event?: unknown) {
		for (const listener of this.listeners.get(name) ?? []) listener(event);
	}
}

class FakeXmlHttpRequest extends FakeEventTarget {
	upload = new FakeEventTarget();
	status = 0;
	response: unknown = null;
	responseType = '';
	withCredentials = false;
	method = '';
	url = '';
	sentBody: Document | XMLHttpRequestBodyInit | null = null;

	open(method: string, url: string) {
		this.method = method;
		this.url = url;
	}

	send(body: Document | XMLHttpRequestBodyInit | null) {
		this.sentBody = body;
	}

	abort() {
		this.emit('abort');
	}

	emitUpload(name: string, event?: unknown) {
		this.upload.emit(name, event);
	}
}
