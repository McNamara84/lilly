import 'fake-indexeddb/auto';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { fetchCollection, updateCollectionEntry } from '#lib/api/collection.js';
import * as database from '#lib/offline/database.js';
import { changeOfflineSession } from '#lib/offline/session.js';
import { entry, profile, snapshot } from './fixtures/offline';

describe('collection responses across a session change', () => {
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
