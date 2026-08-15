import { readFileSync, statSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';
import { isCacheableNavigationPath, isPrivateCachePath } from '$lib/offline/cache-policy';

describe('PWA assets and cache policy', () => {
	it('provides a standalone manifest with all required icons', () => {
		const manifestPath = resolve(process.cwd(), 'static/manifest.webmanifest');
		const manifest = JSON.parse(readFileSync(manifestPath, 'utf8')) as {
			display: string;
			start_url: string;
			icons: { src: string; sizes: string; purpose?: string }[];
		};
		expect(manifest.display).toBe('standalone');
		expect(manifest.start_url).toBe('/collection');
		expect(manifest.icons.map((icon) => icon.sizes)).toEqual(['192x192', '512x512', '512x512']);
		expect(manifest.icons.some((icon) => icon.purpose === 'maskable')).toBe(true);
		for (const icon of manifest.icons) {
			expect(statSync(resolve(process.cwd(), `static${icon.src}`)).size).toBeGreaterThan(100);
		}
	});

	it('never caches APIs or private media but allows shared reference covers', () => {
		expect(isPrivateCachePath('/api/v1/me/collection')).toBe(true);
		expect(isPrivateCachePath('/api/v1/entries/1/photos')).toBe(true);
		expect(isPrivateCachePath('/media/users/1/avatar.webp')).toBe(true);
		expect(isPrivateCachePath('/media/collection/1/private.webp')).toBe(true);
		expect(isPrivateCachePath('/media/covers/series-1/42.webp')).toBe(false);
	});

	it('limits offline navigation caching to the app shell routes', () => {
		expect(isCacheableNavigationPath('/')).toBe(true);
		expect(isCacheableNavigationPath('/login')).toBe(true);
		expect(isCacheableNavigationPath('/collection')).toBe(true);
		expect(isCacheableNavigationPath('/collection/add')).toBe(true);
		expect(isCacheableNavigationPath('/messages')).toBe(false);
		expect(isCacheableNavigationPath('/users/2/collection')).toBe(false);
	});
});
