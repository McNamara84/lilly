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
		vi.spyOn(database, 'mergeOnlineCollectionEntries').mockRejectedValueOnce(
			new Error('Queue unavailable')
		);
		vi.spyOn(database, 'getCachedProfile').mockImplementationOnce(async () => {
			changeOfflineSession(1, true);
			return profile(1);
		});
		const merge = vi.spyOn(database, 'mergeCollectionEntries');
		await expect(fetchCollection()).rejects.toThrow('Sitzung');
		expect(merge).not.toHaveBeenCalled();
	});

	it('returns online results without reading the profile or the full clean collection', async () => {
		const result = { data: [entry], total: 1, page: 1, per_page: 50 };
		vi.stubGlobal('fetch', vi.fn().mockResolvedValue(Response.json(result)));
		const profileLookup = vi
			.spyOn(database, 'getCachedProfile')
			.mockReturnValue(new Promise(() => {}));
		const collectionLookup = vi.spyOn(database, 'listStoredCollectionEntries');
		const merge = vi.spyOn(database, 'mergeOnlineCollectionEntries');
		await expect(fetchCollection()).resolves.toEqual(result);
		expect(profileLookup).not.toHaveBeenCalled();
		expect(collectionLookup).not.toHaveBeenCalled();
		expect(merge).toHaveBeenCalledWith(1, [entry]);
	});

	it('rejects a session change while the atomic page merge is waiting', async () => {
		const probe = Promise.withResolvers<boolean>();
		vi.stubGlobal('fetch', vi.fn().mockResolvedValue(Response.json({ data: [entry], total: 1 })));
		const lookup = vi
			.spyOn(database, 'mergeOnlineCollectionEntries')
			.mockReturnValueOnce(probe.promise);
		const merge = vi.spyOn(database, 'mergeCollectionEntries');
		const request = fetchCollection();
		await vi.waitFor(() => expect(lookup).toHaveBeenCalledOnce());
		changeOfflineSession(2, false);
		probe.resolve(false);
		await expect(request).rejects.toThrow('Sitzung');
		expect(merge).not.toHaveBeenCalled();
	});

	it('keeps durable pending changes visible in a confirmed online session', async () => {
		const { queueCollectionUpdate } = await import('#lib/offline/queue.js');
		await queueCollectionUpdate(1, entry.id, { notes: 'Unsent local note' });
		vi.stubGlobal(
			'fetch',
			vi.fn().mockResolvedValue(Response.json({ data: [entry], total: 1, page: 1, per_page: 50 }))
		);
		const result = await fetchCollection();
		expect(result.data[0].notes).toBe('Unsent local note');
		expect(await database.listMutations(1)).toHaveLength(1);
	});

	it('keeps a stored conflict visible even when no mutation is waiting to be sent', async () => {
		const { queueCollectionUpdate } = await import('#lib/offline/queue.js');
		await queueCollectionUpdate(1, entry.id, { notes: 'Conflict note' });
		const [mutation] = await database.listMutations(1);
		await database.putConflict({
			mutation_id: mutation.mutation_id,
			user_id: 1,
			created_at: mutation.created_at,
			mutation,
			server_entry: entry,
			error: 'Revision conflict',
			code: 'revision_conflict',
			status: 'conflict'
		});
		await database.deleteMutation(mutation.mutation_id);
		await database.mergeCollectionEntries(1, [
			{ ...entry, notes: 'Conflict note', sync_state: 'conflict' }
		]);
		vi.stubGlobal(
			'fetch',
			vi.fn().mockResolvedValue(Response.json({ data: [entry], total: 1, page: 1, per_page: 50 }))
		);
		const result = await fetchCollection();
		expect(result.data[0].notes).toBe('Conflict note');
		expect(result.data[0].sync_state).toBe('conflict');
		expect(await database.listConflicts(1)).toHaveLength(1);
	});

	it('checks cancellation after atomically merging the server page', async () => {
		const controller = new AbortController();
		const probe = Promise.withResolvers<boolean>();
		const lookup = vi
			.spyOn(database, 'mergeOnlineCollectionEntries')
			.mockReturnValueOnce(probe.promise);
		const merge = vi.spyOn(database, 'mergeCollectionEntries');
		vi.stubGlobal('fetch', vi.fn().mockResolvedValue(Response.json({ data: [entry], total: 1 })));
		const request = fetchCollection({}, controller.signal);
		await vi.waitFor(() => expect(lookup).toHaveBeenCalledOnce());
		controller.abort();
		probe.resolve(false);
		await expect(request).rejects.toMatchObject({ name: 'AbortError' });
		expect(merge).not.toHaveBeenCalled();
	});

	it('does not treat an aborted WebKit-style load error as an offline search', async () => {
		const controller = new AbortController();
		vi.stubGlobal(
			'fetch',
			vi.fn(async () => {
				controller.abort();
				throw new TypeError('Load failed');
			})
		);
		const profileLookup = vi.spyOn(database, 'getCachedProfile');
		await expect(fetchCollection({}, controller.signal)).rejects.toMatchObject({
			name: 'AbortError'
		});
		expect(profileLookup).not.toHaveBeenCalled();
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
