import 'fake-indexeddb/auto';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import {
	listConflicts,
	listMutations,
	queueCollectionCreate,
	queueCollectionUpdate,
	readLocalCollection,
	reapplyConflictLocalVersion,
	acceptConflictServerVersion,
	synchronizeCollection
} from '$lib/offline/collection';
import {
	listStoredCollectionEntries,
	replaceOfflineSnapshot,
	resetOfflineDatabaseForTests
} from '$lib/offline/database';
import type { CollectionSyncResponse } from '$lib/offline/types';
import { entry, snapshot } from './fixtures/offline';

describe('offline collection queue', () => {
	beforeEach(async () => {
		await resetOfflineDatabaseForTests();
		await replaceOfflineSnapshot(snapshot(1));
	});

	it('filters cached entries with the same pagination semantics', async () => {
		await replaceOfflineSnapshot(
			snapshot(1, [
				entry,
				{ ...entry, id: 31, issue_id: 21, issue_number: 2, title: 'Nacht über Berlin' }
			])
		);
		const result = await readLocalCollection(1, { q: 'Gott', page: 1, per_page: 1 });

		expect(result.total).toBe(1);
		expect(result.data[0].id).toBe(30);
		expect(result.per_page).toBe(1);
	});

	it('merges edits into a pending create and syncs it exactly once', async () => {
		const created = await queueCollectionCreate(1, {
			issue_id: 20,
			condition_grade: 'Z2',
			status: 'owned'
		});
		await queueCollectionUpdate(1, created.entry.id, {
			condition_grade: 'Z1',
			notes: 'offline geändert'
		});

		const pending = await listMutations(1);
		expect(pending).toHaveLength(1);
		expect(pending[0].operation).toBe('create');
		if (pending[0].operation === 'create') {
			expect(pending[0].entry.condition_grade).toBe('Z1');
			expect(pending[0].entry.notes).toBe('offline geändert');
		}

		const serverEntry = { ...entry, id: 99, revision: 1, condition_grade: 'Z1' as const };
		const send = vi.fn(async (): Promise<CollectionSyncResponse> => ({
			results: [
				{
					mutation_id: pending[0].mutation_id,
					status: 'applied',
					entry: serverEntry,
					error: null,
					code: null
				}
			]
		}));
		await synchronizeCollection(1, send);

		expect(send).toHaveBeenCalledOnce();
		expect(await listMutations(1)).toEqual([]);
		expect(
			(await listStoredCollectionEntries(1)).some((item) => item.id === created.entry.id)
		).toBe(false);
		expect((await listStoredCollectionEntries(1)).find((item) => item.id === 99)?.sync_state).toBe(
			'synced'
		);
	});

	it('parks a stale update until the user chooses a resolution', async () => {
		const queued = await queueCollectionUpdate(1, entry.id, { condition_grade: 'Z1' });
		const serverEntry = { ...entry, revision: 2, condition_grade: 'Z3' as const };
		await synchronizeCollection(1, async () => ({
			results: [
				{
					mutation_id: queued.mutation.mutation_id,
					status: 'conflict',
					entry: serverEntry,
					error: 'Serverstand ist neuer',
					code: 'collection_revision_conflict'
				}
			]
		}));

		const [conflict] = await listConflicts(1);
		expect(conflict.server_entry?.revision).toBe(2);
		expect((await listStoredCollectionEntries(1))[0].sync_state).toBe('conflict');
		expect(await listMutations(1)).toEqual([]);

		const reapplied = await reapplyConflictLocalVersion(conflict);
		expect(reapplied.operation).toBe('update');
		if (reapplied.operation === 'update') expect(reapplied.base_revision).toBe(2);
		expect(await listConflicts(1)).toEqual([]);
	});

	it('can explicitly accept the server version', async () => {
		const queued = await queueCollectionUpdate(1, entry.id, { condition_grade: 'Z1' });
		const serverEntry = { ...entry, revision: 2, condition_grade: 'Z4' as const };
		await synchronizeCollection(1, async () => ({
			results: [
				{
					mutation_id: queued.mutation.mutation_id,
					status: 'conflict',
					entry: serverEntry,
					error: 'Konflikt',
					code: 'collection_revision_conflict'
				}
			]
		}));
		const [conflict] = await listConflicts(1);
		await acceptConflictServerVersion(conflict);

		expect(await listConflicts(1)).toEqual([]);
		expect((await listStoredCollectionEntries(1))[0]).toMatchObject({
			condition_grade: 'Z4',
			revision: 2,
			sync_state: 'synced'
		});
	});

	it('keeps transient failures queued and counts attempts', async () => {
		await queueCollectionUpdate(1, entry.id, { notes: 'bleibt lokal' });
		await expect(
			synchronizeCollection(1, async () => {
				throw new TypeError('Network unavailable');
			})
		).rejects.toThrow('Network unavailable');

		const [pending] = await listMutations(1);
		expect(pending.attempts).toBe(1);
		expect(pending.last_error).toBe('Network unavailable');
	});
});
