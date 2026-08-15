import { afterEach, describe, expect, it, vi } from 'vitest';

describe('offline database initialization failures', () => {
	afterEach(() => {
		vi.unstubAllGlobals();
		vi.resetModules();
	});

	it('reports an unavailable IndexedDB implementation', async () => {
		vi.stubGlobal('indexedDB', undefined);
		const database = await import('$lib/offline/database');

		await expect(database.getCachedProfile()).rejects.toThrow('IndexedDB is not available');
	});

	it('clears the cached open promise after an IndexedDB open error', async () => {
		const open = vi.fn(() => {
			const request = new EventTarget() as IDBOpenDBRequest;
			Object.defineProperty(request, 'error', {
				value: new DOMException('Database unavailable', 'UnknownError')
			});
			queueMicrotask(() => request.dispatchEvent(new Event('error')));
			return request;
		});
		vi.stubGlobal('indexedDB', { open });
		const database = await import('$lib/offline/database');

		await expect(database.getCachedProfile()).rejects.toThrow('Database unavailable');
		await expect(database.getCachedProfile()).rejects.toThrow('Database unavailable');
		expect(open).toHaveBeenCalledTimes(2);
	});
});
