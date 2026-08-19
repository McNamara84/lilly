import { beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('$service-worker', () => ({
	build: ['/_app/immutable/app.js'],
	files: ['/manifest.webmanifest', '/api/private', '/media/private', '/manifest.webmanifest'],
	version: 'test'
}));

type WorkerHandler = (event: unknown) => void;

function createCache() {
	const entries = new Map<string, Response>();
	const key = (request: Request | string) =>
		typeof request === 'string' ? request : new URL(request.url).toString();

	return {
		entries,
		addAll: vi.fn(async () => undefined),
		match: vi.fn(async (request: Request | string) => entries.get(key(request))),
		put: vi.fn(async (request: Request | string, response: Response) => {
			entries.set(key(request), response);
		})
	};
}

describe('service worker', () => {
	let handlers: Map<string, WorkerHandler>;
	let appCache: ReturnType<typeof createCache>;
	let coverCache: ReturnType<typeof createCache>;
	let fetchMock: ReturnType<typeof vi.fn>;
	let worker: {
		location: { origin: string };
		clients: { claim: ReturnType<typeof vi.fn> };
		skipWaiting: ReturnType<typeof vi.fn>;
		addEventListener: ReturnType<typeof vi.fn>;
	};
	let cacheStorage: {
		open: ReturnType<typeof vi.fn>;
		keys: ReturnType<typeof vi.fn>;
		delete: ReturnType<typeof vi.fn>;
	};

	beforeEach(async () => {
		vi.resetModules();
		handlers = new Map();
		appCache = createCache();
		coverCache = createCache();
		fetchMock = vi.fn();
		worker = {
			location: { origin: 'https://lilly.test' },
			clients: { claim: vi.fn(async () => undefined) },
			skipWaiting: vi.fn(async () => undefined),
			addEventListener: vi.fn((type: string, handler: WorkerHandler) => handlers.set(type, handler))
		};
		cacheStorage = {
			open: vi.fn(async (name: string) => (name === 'lilly-covers-test' ? coverCache : appCache)),
			keys: vi.fn(async () => ['lilly-app-old', 'lilly-covers-test', 'third-party']),
			delete: vi.fn(async () => true)
		};

		vi.stubGlobal('self', worker);
		vi.stubGlobal('caches', cacheStorage);
		vi.stubGlobal('fetch', fetchMock);
		await import('../src/service-worker');
	});

	function dispatchExtendable(type: 'install' | 'activate') {
		let completion: Promise<unknown> | undefined;
		handlers.get(type)?.({ waitUntil: (promise: Promise<unknown>) => (completion = promise) });
		return completion;
	}

	function dispatchFetch(request: Request) {
		let response: Promise<Response> | undefined;
		handlers.get('fetch')?.({
			request,
			respondWith: (promise: Promise<Response>) => (response = promise)
		});
		return response;
	}

	function dispatchMessage(data: unknown) {
		let completion: Promise<unknown> | undefined;
		handlers.get('message')?.({
			data,
			waitUntil: (promise: Promise<unknown>) => (completion = promise)
		});
		return completion;
	}

	it('precaches public assets and tolerates an unavailable app shell', async () => {
		fetchMock.mockResolvedValueOnce(new Response('shell'));
		await dispatchExtendable('install');

		expect(appCache.addAll).toHaveBeenCalledWith(['/manifest.webmanifest']);
		expect(appCache.put).toHaveBeenCalledWith('/', expect.any(Response));

		fetchMock.mockRejectedValueOnce(new TypeError('offline'));
		await expect(dispatchExtendable('install')).resolves.toBeUndefined();
	});

	it('removes only obsolete Lilly caches during activation', async () => {
		await dispatchExtendable('activate');

		expect(cacheStorage.delete).toHaveBeenCalledOnce();
		expect(cacheStorage.delete).toHaveBeenCalledWith('lilly-app-old');
		expect(worker.clients.claim).toHaveBeenCalledOnce();
	});

	it('uses network-first navigation with cached page and shell fallbacks', async () => {
		const request = new Request('https://lilly.test/collection');
		Object.defineProperty(request, 'mode', { value: 'navigate' });
		fetchMock.mockResolvedValueOnce(new Response('fresh'));
		await expect(dispatchFetch(request)).resolves.toHaveProperty('status', 200);
		expect(appCache.put).toHaveBeenCalledWith(request, expect.any(Response));

		appCache.entries.set(request.url, new Response('cached page'));
		fetchMock.mockRejectedValueOnce(new TypeError('offline'));
		expect(await (await dispatchFetch(request))?.text()).toBe('cached page');

		appCache.entries.delete(request.url);
		appCache.entries.set('/', new Response('cached shell'));
		fetchMock.mockRejectedValueOnce(new TypeError('offline'));
		expect(await (await dispatchFetch(request))?.text()).toBe('cached shell');

		appCache.entries.clear();
		fetchMock.mockRejectedValueOnce(new TypeError('offline'));
		await expect(dispatchFetch(request)).rejects.toThrow('offline');
	});

	it('serves cover images stale-while-revalidate and app assets cache-first', async () => {
		const cover = new Request('https://lilly.test/media/covers/maddrax/1.webp');
		coverCache.entries.set(cover.url, new Response('old cover'));
		fetchMock.mockResolvedValueOnce(new Response('new cover'));
		expect(await (await dispatchFetch(cover))?.text()).toBe('old cover');
		await vi.waitFor(() => expect(coverCache.put).toHaveBeenCalled());

		const cachedAsset = new Request('https://lilly.test/_app/immutable/app.js');
		appCache.entries.set(cachedAsset.url, new Response('cached asset'));
		expect(await (await dispatchFetch(cachedAsset))?.text()).toBe('cached asset');

		const uncachedAsset = new Request('https://lilly.test/_app/immutable/chunk.js');
		fetchMock.mockResolvedValueOnce(new Response('new asset'));
		expect(await (await dispatchFetch(uncachedAsset))?.text()).toBe('new asset');
		expect(appCache.put).toHaveBeenCalledWith(uncachedAsset, expect.any(Response));
	});

	it('keeps a cached cover when background revalidation fails', async () => {
		const cover = new Request('https://lilly.test/media/covers/maddrax/2.webp');
		coverCache.entries.set(cover.url, new Response('cached cover'));
		fetchMock.mockRejectedValueOnce(new TypeError('offline'));

		expect(await (await dispatchFetch(cover))?.text()).toBe('cached cover');
		await vi.waitFor(() => expect(fetchMock).toHaveBeenCalledWith(cover));
	});

	it('does not intercept private, cross-origin, non-GET or unrelated requests', () => {
		expect(dispatchFetch(new Request('https://lilly.test/api/v1/me'))).toBeUndefined();
		expect(dispatchFetch(new Request('https://other.test/image.webp'))).toBeUndefined();
		expect(
			dispatchFetch(new Request('https://lilly.test/collection', { method: 'POST' }))
		).toBeUndefined();
		expect(dispatchFetch(new Request('https://lilly.test/favicon.ico'))).toBeUndefined();
	});

	it('activates a waiting worker on request', () => {
		handlers.get('message')?.({ data: 'IGNORE' });
		handlers.get('message')?.({ data: 'SKIP_WAITING' });
		expect(worker.skipWaiting).toHaveBeenCalledOnce();
	});

	it('purges all Lilly caches after account deactivation', async () => {
		await dispatchMessage({ type: 'PURGE_PRIVATE_DATA' });

		expect(cacheStorage.delete).toHaveBeenCalledWith('lilly-app-old');
		expect(cacheStorage.delete).toHaveBeenCalledWith('lilly-covers-test');
		expect(cacheStorage.delete).not.toHaveBeenCalledWith('third-party');
	});
});
