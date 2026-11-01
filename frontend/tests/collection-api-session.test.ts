import 'fake-indexeddb/auto';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { addToCollection, fetchCollection, updateCollectionEntry } from '#lib/api/collection.js';
import * as database from '#lib/offline/database.js';
import * as offlineCollection from '#lib/offline/collection.js';
import { changeOfflineSession } from '#lib/offline/session.js';
import { entry, profile, snapshot } from './fixtures/offline';

describe('collection responses across a session change', () => {
	const writes = {
		POST: () => addToCollection({ issue_id: entry.issue_id, notes: 'private create' }),
		PATCH: () => updateCollectionEntry(entry.id, { notes: 'private update' })
	};
	for (const method of ['POST', 'PATCH'] as const) {
		it(`does not synchronize a queued ${method} after another login takes over`, async () => {
			const fetch = vi.fn();
			vi.stubGlobal('fetch', fetch);
			const name = method === 'POST' ? 'queueCollectionCreate' : 'queueCollectionUpdate';
			const queue = offlineCollection[name];
			vi.spyOn(offlineCollection, name).mockImplementationOnce(async (...args: unknown[]) => {
				const queued = await (queue as (...args: unknown[]) => Promise<unknown>)(...args);
				changeOfflineSession(2, false);
				return queued as Awaited<ReturnType<typeof queue>>;
			});
			await expect(writes[method]()).rejects.toThrow('Sitzung');
			expect(fetch).not.toHaveBeenCalled();
		});

		it.each(['success', 'network failure'])(
			`discards a queued ${method} result when its synchronization ends in %s after logout`,
			async (outcome) => {
				vi.spyOn(offlineCollection, 'synchronizeCollection').mockImplementationOnce(async () => {
					changeOfflineSession(1, true);
					if (outcome === 'network failure') throw new TypeError('Network unavailable');
					return null;
				});
				await expect(writes[method]()).rejects.toThrow('Sitzung');
			}
		);
		it.each(['missing', 'unavailable', 'cached'] as const)(
			`does not send ${method} when a newer login supersedes a delayed %s profile lookup`,
			async (cache) => {
				const lookup = Promise.withResolvers<ReturnType<typeof profile> | null>();
				vi.spyOn(database, 'getCachedProfile').mockReturnValueOnce(lookup.promise);
				const fetch = vi.fn().mockResolvedValue(Response.json(entry));
				vi.stubGlobal('fetch', fetch);
				const pending = writes[method]();
				changeOfflineSession(2, false);
				if (cache === 'unavailable') lookup.reject(new Error('IndexedDB unavailable'));
				else lookup.resolve(cache === 'cached' ? profile(1) : null);
				await expect(pending).rejects.toThrow('Sitzung');
				expect(fetch).not.toHaveBeenCalled();
				expect(await database.listMutations(1)).toEqual([]);
			}
		);

		it(`refuses ${method} after local logout even if server cookies remain valid`, async () => {
			const fetch = vi.fn().mockResolvedValue(Response.json(entry));
			vi.stubGlobal('fetch', fetch);
			changeOfflineSession(1, true);
			await expect(writes[method]()).rejects.toThrow('Sitzung');
			expect(fetch).not.toHaveBeenCalled();
		});

		it.each(['missing', 'unavailable'] as const)(
			`discards a ${method} response after logout with a %s profile`,
			async (cache) => {
				const lookup = vi.spyOn(database, 'getCachedProfile');
				if (cache === 'missing') lookup.mockResolvedValue(null);
				else lookup.mockRejectedValue(new Error('IndexedDB unavailable'));
				const response = Promise.withResolvers<Response>();
				const fetch = vi.fn(() => response.promise);
				vi.stubGlobal('fetch', fetch);
				const pending = writes[method]();
				await vi.waitFor(() => expect(fetch).toHaveBeenCalledOnce());
				changeOfflineSession(1, true);
				response.resolve(Response.json(entry));
				await expect(pending).rejects.toThrow('Sitzung');
			}
		);
	}

	it.each(['missing', 'unavailable', 'cached'] as const)(
		'does not send a PATCH or queue an edit after a delayed %s entry lookup changes session',
		async (cache) => {
			const lookup = Promise.withResolvers<(typeof entry)[]>();
			vi.spyOn(database, 'listStoredCollectionEntries').mockReturnValueOnce(lookup.promise);
			const fetch = vi.fn().mockResolvedValue(Response.json(entry));
			vi.stubGlobal('fetch', fetch);
			const pending = updateCollectionEntry(entry.id, { notes: 'stale edit' });
			await vi.waitFor(() => expect(database.listStoredCollectionEntries).toHaveBeenCalled());
			changeOfflineSession(2, false);
			if (cache === 'unavailable') lookup.reject(new Error('IndexedDB unavailable'));
			else lookup.resolve(cache === 'cached' ? [entry] : []);
			await expect(pending).rejects.toThrow('Sitzung');
			expect(fetch).not.toHaveBeenCalled();
			expect(await database.listMutations(1)).toEqual([]);
		}
	);
	it.each(['missing', 'unavailable'] as const)(
		'discards a PATCH response after logout when the cached profile is %s',
		async (cache) => {
			const lookup = vi.spyOn(database, 'getCachedProfile');
			if (cache === 'missing') lookup.mockResolvedValue(null);
			else lookup.mockRejectedValue(new Error('IndexedDB unavailable'));
			const response = Promise.withResolvers<Response>();
			const fetch = vi.fn(() => response.promise);
			vi.stubGlobal('fetch', fetch);
			const pending = updateCollectionEntry(entry.id, { notes: 'private response' });
			await vi.waitFor(() => expect(fetch).toHaveBeenCalledOnce());
			changeOfflineSession(1, true);
			changeOfflineSession(2, false);
			response.resolve(Response.json({ ...entry, notes: 'private response' }));
			await expect(pending).rejects.toThrow('Sitzung');
		}
	);
	beforeEach(async () => {
		localStorage.clear();
		await database.resetOfflineDatabaseForTests();
		changeOfflineSession(1, false);
		await database.saveConfirmedProfile(profile(1));
		await database.replaceOfflineSnapshot(snapshot(1));
	});
	afterEach(() => {
		vi.restoreAllMocks();
		vi.unstubAllGlobals();
	});

	it('discards a collection response when logout interrupts the cached profile lookup', async () => {
		vi.stubGlobal('fetch', vi.fn().mockResolvedValue(Response.json({ data: [entry], total: 1 })));
		vi.spyOn(database, 'getCachedProfile').mockImplementationOnce(async () => {
			changeOfflineSession(1, true);
			return profile(1);
		});
		const merge = vi.spyOn(database, 'mergeCollectionEntries');
		await expect(fetchCollection()).rejects.toThrow('Sitzung');
		expect(merge).not.toHaveBeenCalled();
	});

	it('discards an online update to an uncached entry if the account changed in flight', async () => {
		vi.stubGlobal(
			'fetch',
			vi.fn(async () => {
				changeOfflineSession(2, false);
				return Response.json({ ...entry, id: 99, notes: 'private reply' });
			})
		);
		await expect(updateCollectionEntry(99, { notes: 'private reply' })).rejects.toThrow('Sitzung');
		expect(await database.listStoredCollectionEntries(1)).toEqual([entry]);
		expect(await database.listStoredCollectionEntries(2)).toEqual([]);
	});
});
