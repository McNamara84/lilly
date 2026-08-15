import 'fake-indexeddb/auto';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import {
	addToCollection,
	fetchCollection,
	syncPendingCollectionChanges
} from '$lib/api/collection';
import {
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
});
