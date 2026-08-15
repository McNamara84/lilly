import 'fake-indexeddb/auto';
import { beforeEach, describe, expect, it } from 'vitest';
import {
	clearOfflineUserData,
	getCachedProfile,
	getSnapshotTimestamp,
	listMutations,
	listStoredCollectionEntries,
	listStoredIssues,
	listStoredSeries,
	replaceOfflineSnapshot,
	resetOfflineDatabaseForTests,
	saveConfirmedProfile,
	enqueueMutation
} from '$lib/offline/database';
import type { CollectionMutation } from '$lib/offline/types';
import { entry, issue, profile, series, snapshot } from './fixtures/offline';

describe('offline database', () => {
	beforeEach(async () => {
		await resetOfflineDatabaseForTests();
	});

	it('stores the last confirmed profile and replaces a snapshot atomically', async () => {
		await saveConfirmedProfile(profile(1));
		await replaceOfflineSnapshot(snapshot(1));

		expect(await getCachedProfile()).toEqual(profile(1));
		expect(await listStoredSeries(1)).toEqual([series]);
		expect(await listStoredIssues(1)).toEqual([issue]);
		expect(await listStoredCollectionEntries(1)).toEqual([entry]);
		expect(await getSnapshotTimestamp(1)).toBe('2026-08-15T01:02:03Z');

		await replaceOfflineSnapshot(snapshot(1, []));
		expect(await listStoredCollectionEntries(1)).toEqual([]);
	});

	it('keeps private records separated by user and removes only the logged-out user', async () => {
		await saveConfirmedProfile(profile(1));
		await replaceOfflineSnapshot(snapshot(1));
		await saveConfirmedProfile(profile(2));
		await replaceOfflineSnapshot(snapshot(2, [{ ...entry, id: 31 }]));

		expect((await listStoredCollectionEntries(1))[0].id).toBe(30);
		expect((await listStoredCollectionEntries(2))[0].id).toBe(31);

		await clearOfflineUserData(2);
		expect(await getCachedProfile()).toBeNull();
		expect((await listStoredCollectionEntries(1))[0].id).toBe(30);
		expect(await listStoredCollectionEntries(2)).toEqual([]);
	});

	it('persists mutations in creation order', async () => {
		const later: CollectionMutation = {
			mutation_id: '11111111-1111-4111-8111-111111111111',
			user_id: 1,
			operation: 'create',
			temp_entry_id: -1,
			entry: { issue_id: 20, status: 'wanted' },
			created_at: '2026-08-15T02:00:00Z',
			attempts: 0,
			last_error: null
		};
		const earlier: CollectionMutation = {
			...later,
			mutation_id: '22222222-2222-4222-8222-222222222222',
			temp_entry_id: -2,
			created_at: '2026-08-15T01:00:00Z'
		};
		await enqueueMutation(later);
		await enqueueMutation(earlier);

		expect((await listMutations(1)).map((item) => item.mutation_id)).toEqual([
			earlier.mutation_id,
			later.mutation_id
		]);
	});
});
