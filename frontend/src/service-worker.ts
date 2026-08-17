/// <reference lib="webworker" />

import { build, files, version } from '$service-worker';
import { isCacheableNavigationPath, isPrivateCachePath } from '$lib/offline/cache-policy';

const worker = self as unknown as ServiceWorkerGlobalScope;
const APP_CACHE = `lilly-app-${version}`;
const COVER_CACHE = `lilly-covers-${version}`;
const ALLOWED_CACHES = new Set([APP_CACHE, COVER_CACHE]);
const PRECACHE_ASSETS = [...new Set(files)].filter(
	(path) => !path.startsWith('/api/') && !path.startsWith('/media/')
);
const BUILD_ASSETS = new Set(build);

async function networkFirstNavigation(request: Request): Promise<Response> {
	const cache = await caches.open(APP_CACHE);
	try {
		const response = await fetch(request);
		if (response.ok) await cache.put(request, response.clone());
		return response;
	} catch (error) {
		const cached = await cache.match(request, { ignoreSearch: true });
		if (cached) return cached;
		const root = await cache.match('/');
		if (root) return root;
		throw error;
	}
}

async function staleWhileRevalidate(request: Request): Promise<Response> {
	const cache = await caches.open(COVER_CACHE);
	const cached = await cache.match(request);
	const network = fetch(request).then(async (response) => {
		if (response.ok) await cache.put(request, response.clone());
		return response;
	});
	if (!cached) return network;

	// Revalidation runs in the background when a stale response is available.
	// Handle network failures here because respondWith() only observes the
	// cached response in that case.
	void network.catch(() => undefined);
	return cached;
}

async function cacheFirstAppAsset(request: Request): Promise<Response> {
	const cache = await caches.open(APP_CACHE);
	const cached = await cache.match(request);
	if (cached) return cached;
	const response = await fetch(request);
	if (response.ok) await cache.put(request, response.clone());
	return response;
}

worker.addEventListener('install', (event) => {
	event.waitUntil(
		caches.open(APP_CACHE).then(async (cache) => {
			await cache.addAll(PRECACHE_ASSETS);
			try {
				const root = await fetch('/');
				if (root.ok) await cache.put('/', root);
			} catch {
				// The immutable build assets are enough to finish installation.
			}
		})
	);
});

worker.addEventListener('activate', (event) => {
	event.waitUntil(
		(async () => {
			await Promise.all(
				(await caches.keys())
					.filter((key) => key.startsWith('lilly-') && !ALLOWED_CACHES.has(key))
					.map((key) => caches.delete(key))
			);
			await worker.clients.claim();
		})()
	);
});

worker.addEventListener('fetch', (event) => {
	const { request } = event;
	if (request.method !== 'GET') return;

	const url = new URL(request.url);
	if (url.origin !== worker.location.origin || isPrivateCachePath(url.pathname)) return;

	if (request.mode === 'navigate' && isCacheableNavigationPath(url.pathname)) {
		event.respondWith(networkFirstNavigation(request));
		return;
	}

	if (url.pathname.startsWith('/media/covers/')) {
		event.respondWith(staleWhileRevalidate(request));
		return;
	}

	if (BUILD_ASSETS.has(url.pathname) || url.pathname.startsWith('/_app/immutable/')) {
		event.respondWith(cacheFirstAppAsset(request));
	}
});

worker.addEventListener('message', (event) => {
	if (event.data === 'SKIP_WAITING') void worker.skipWaiting();
	if (event.data?.type === 'PURGE_PRIVATE_DATA') {
		event.waitUntil(
			caches
				.keys()
				.then((keys) =>
					Promise.all(
						keys.filter((key) => key.startsWith('lilly-')).map((key) => caches.delete(key))
					)
				)
		);
	}
});
