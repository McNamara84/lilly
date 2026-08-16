import 'fake-indexeddb/auto';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import {
	addToCollection,
	fetchCollection,
	fetchOfflineSnapshot,
	refreshOfflineSnapshot,
	syncPendingCollectionChanges,
	updateCollectionEntry
} from '$lib/api/collection';
import {
	deleteStoredCollectionEntry,
	listMutations,
	listStoredCollectionEntries,
	replaceOfflineSnapshot,
	resetOfflineDatabaseForTests,
	saveConfirmedProfile
} from '$lib/offline/database';
import { entry, profile, snapshot } from './fixtures/offline';

const mockFetch = vi.fn();
vi.stubGlobal('fetch', mockFetch);

function jsonResponse(data: unknown, status = 200): Response {
	return {
		ok: status >= 200 && status < 300,
		status,
		json: () => Promise.resolve(data)
	} as Response;
}

describe('collection API offline integration', () => {
	beforeEach(async () => {
		mockFetch.mockReset();
		await resetOfflineDatabaseForTests();
		await saveConfirmedProfile(profile(1));
		await replaceOfflineSnapshot(snapshot(1));
	});

	it('falls back to the user-scoped collection only for a network failure', async () => {
		mockFetch.mockRejectedValue(new TypeError('Network unavailable'));

		const result = await fetchCollection({ series_slug: 'maddrax' });

		expect(result.data).toEqual([entry]);
		expect(result.total).toBe(1);
	});

	it('merges successful online collection reads into the local cache', async () => {
		const onlineEntry = { ...entry, id: 40, notes: 'online' };
		mockFetch.mockResolvedValue(
			jsonResponse({ data: [onlineEntry, { ...entry, id: -4 }], page: 1, per_page: 50, total: 2 })
		);

		await fetchCollection();

		await vi.waitFor(async () => {
			const stored = await listStoredCollectionEntries(1);
			expect(stored.some(({ id }) => id === 40)).toBe(true);
			expect(stored.some(({ id }) => id === -4)).toBe(false);
		});
	});

	it('persists an offline create and sends it once after reconnect', async () => {
		mockFetch.mockRejectedValueOnce(new TypeError('Network unavailable'));
		const optimistic = await addToCollection({
			issue_id: 20,
			condition_grade: 'Z1',
			status: 'owned',
			notes: 'offline'
		});
		expect(optimistic.id).toBeLessThan(0);
		const [pending] = await listMutations(1);
		expect(pending.attempts).toBe(1);

		const serverEntry = { ...entry, id: 99, condition_grade: 'Z1' as const, revision: 1 };
		mockFetch.mockResolvedValueOnce(
			jsonResponse({
				results: [
					{
						mutation_id: pending.mutation_id,
						status: 'applied',
						entry: serverEntry,
						error: null,
						code: null
					}
				]
			})
		);
		await syncPendingCollectionChanges();

		expect(await listMutations(1)).toEqual([]);
		const stored = await listStoredCollectionEntries(1);
		expect(stored.some((item) => item.id === optimistic.id)).toBe(false);
		expect(stored.find((item) => item.id === 99)?.condition_grade).toBe('Z1');
		expect(mockFetch).toHaveBeenCalledTimes(2);
	});

	it('returns the optimistic create when synchronization has no matching result', async () => {
		mockFetch.mockResolvedValueOnce(jsonResponse({ results: [] }));

		const optimistic = await addToCollection({ issue_id: 20, status: 'wanted' });

		expect(optimistic.id).toBeLessThan(0);
	});

	it('surfaces rejected and non-network create failures', async () => {
		mockFetch.mockImplementationOnce(async (_url, init: RequestInit) => {
			const [mutation] = JSON.parse(String(init.body)).mutations;
			return jsonResponse({
				results: [
					{
						mutation_id: mutation.mutation_id,
						status: 'rejected',
						entry: null,
						error: null,
						code: 'invalid'
					}
				]
			});
		});
		await expect(addToCollection({ issue_id: 20 })).rejects.toThrow('Änderung abgelehnt');

		await replaceOfflineSnapshot(snapshot(1));
		mockFetch.mockResolvedValueOnce(jsonResponse({ error: 'Sync ungültig' }, 400));
		await expect(addToCollection({ issue_id: 20 })).rejects.toThrow('Sync ungültig');
	});

	it('updates a non-cached entry online and stores the response', async () => {
		await deleteStoredCollectionEntry(1, entry.id);
		const online = { ...entry, condition_grade: 'Z0' as const, revision: 2 };
		mockFetch.mockResolvedValueOnce(jsonResponse(online));

		await expect(updateCollectionEntry(entry.id, { condition_grade: 'Z0' })).resolves.toEqual(
			online
		);
		await vi.waitFor(async () =>
			expect((await listStoredCollectionEntries(1))[0].condition_grade).toBe('Z0')
		);
	});

	it('returns the synchronized or optimistic result for cached updates', async () => {
		const serverEntry = { ...entry, notes: 'vom Server', revision: 2 };
		mockFetch.mockImplementationOnce(async (_url, init: RequestInit) => {
			const [mutation] = JSON.parse(String(init.body)).mutations;
			return jsonResponse({
				results: [
					{
						mutation_id: mutation.mutation_id,
						status: 'applied',
						entry: serverEntry,
						error: null,
						code: null
					}
				]
			});
		});
		await expect(updateCollectionEntry(entry.id, { notes: 'lokal' })).resolves.toEqual(serverEntry);

		await replaceOfflineSnapshot(snapshot(1));
		mockFetch.mockResolvedValueOnce(jsonResponse({ results: [] }));
		await expect(updateCollectionEntry(entry.id, { notes: 'optimistisch' })).resolves.toMatchObject(
			{
				notes: 'optimistisch',
				sync_state: 'pending'
			}
		);
	});

	it('keeps cached updates on network failure and surfaces rejected updates', async () => {
		mockFetch.mockRejectedValueOnce(new TypeError('Network unavailable'));
		await expect(updateCollectionEntry(entry.id, { notes: 'offline' })).resolves.toMatchObject({
			notes: 'offline',
			sync_state: 'pending'
		});

		await resetOfflineDatabaseForTests();
		await saveConfirmedProfile(profile(1));
		await replaceOfflineSnapshot(snapshot(1));
		mockFetch.mockImplementationOnce(async (_url, init: RequestInit) => {
			const [mutation] = JSON.parse(String(init.body)).mutations;
			return jsonResponse({
				results: [
					{
						mutation_id: mutation.mutation_id,
						status: 'rejected',
						entry: null,
						error: null,
						code: 'invalid'
					}
				]
			});
		});
		await expect(updateCollectionEntry(entry.id, { notes: 'invalid' })).rejects.toThrow(
			'Änderung abgelehnt'
		);
	});

	it('downloads and atomically refreshes the offline snapshot', async () => {
		const replacement = snapshot(1, [{ ...entry, id: 88 }]);
		mockFetch.mockResolvedValueOnce(jsonResponse(replacement));
		await expect(fetchOfflineSnapshot()).resolves.toEqual(replacement);
		expect(mockFetch).toHaveBeenLastCalledWith('/api/v1/me/collection/offline-snapshot', {
			credentials: 'same-origin'
		});

		mockFetch.mockResolvedValueOnce(jsonResponse(replacement));
		await refreshOfflineSnapshot();
		expect((await listStoredCollectionEntries(1))[0].id).toBe(88);
	});
});
