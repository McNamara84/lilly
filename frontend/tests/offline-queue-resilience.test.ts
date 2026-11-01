import 'fake-indexeddb/auto';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import {
	editOfflineState,
	listConflicts,
	listMutations,
	listStoredCollectionEntries,
	mergeCollectionEntries,
	replaceOfflineSnapshot,
	resetOfflineDatabaseForTests
} from '#lib/offline/database.js';
import {
	queueCollectionCreate,
	queueCollectionUpdate,
	synchronizeCollection,
	reapplyConflictLocalVersion
} from '#lib/offline/queue.js';
import { entry, issue, snapshot } from './fixtures/offline';
import type { CollectionSyncResponse } from '#lib/offline/types.js';

function applied(mutations: Record<string, unknown>[]): CollectionSyncResponse {
	return {
		results: mutations.map((mutation, index) => ({
			mutation_id: String(mutation.mutation_id),
			status: 'applied',
			entry: {
				...entry,
				id: Number(mutation.entry_id ?? 1000 + index),
				revision: Number(mutation.base_revision ?? 0) + 1,
				...((mutation.entry as object) ?? (mutation.changes as object))
			},
			code: null,
			error: null
		}))
	};
}

describe('durable offline transactions', () => {
	beforeEach(async () => {
		localStorage.clear();
		await resetOfflineDatabaseForTests();
		await replaceOfflineSnapshot(snapshot(1));
	});

	it('rolls back all stores when a transaction callback fails', async () => {
		await expect(
			editOfflineState(1, (state) => {
				state.entries.clear();
				state.metadata.set('bad', { key: 'bad', user_id: 1, value: 'not committed' });
				throw new Error('interrupted');
			})
		).rejects.toThrow('interrupted');
		expect(await listStoredCollectionEntries(1)).toEqual([entry]);
		expect(await editOfflineState(1, (state) => state.metadata.has('bad'))).toBe(false);
	});

	it('sends 101 changes in bounded batches and resumes only the failed batch', async () => {
		for (let index = 0; index < 101; index++)
			await queueCollectionCreate(1, { issue_id: issue.id, edition_label: `copy ${index}` });
		const send = vi
			.fn()
			.mockImplementationOnce(async (mutations) => applied(mutations))
			.mockRejectedValueOnce(new TypeError('connection lost'));
		await expect(synchronizeCollection(1, send)).rejects.toThrow('connection lost');
		expect(send.mock.calls.map(([mutations]) => mutations.length)).toEqual([100, 1]);
		expect(await listMutations(1)).toHaveLength(1);
		const retry = vi.fn(async (mutations: Record<string, unknown>[]) => applied(mutations));
		await synchronizeCollection(1, retry);
		expect(retry.mock.calls[0][0]).toEqual(send.mock.calls[1][0]);
		expect(await listMutations(1)).toEqual([]);
	});

	it('keeps exactly 100 mutations in one request', async () => {
		for (let index = 0; index < 100; index++)
			await queueCollectionCreate(1, { issue_id: issue.id });
		const send = vi.fn(async (mutations: Record<string, unknown>[]) => applied(mutations));
		await synchronizeCollection(1, send);
		expect(send).toHaveBeenCalledOnce();
		expect(send.mock.calls[0][0]).toHaveLength(100);
	});

	it('never edits an attempted payload and rebases a subsequent update after a lost response', async () => {
		const created = await queueCollectionCreate(1, { issue_id: issue.id, notes: 'first intent' });
		const lost = vi.fn(async () => {
			throw new TypeError('lost response');
		});
		await expect(synchronizeCollection(1, lost)).rejects.toThrow('lost response');
		await queueCollectionUpdate(1, created.entry.id, {
			notes: 'later intent',
			condition_grade: 'Z4'
		});
		const pending = await listMutations(1);
		expect(pending).toHaveLength(2);
		expect(pending.find((item) => item.operation === 'create')).toMatchObject({
			sent: true,
			entry: { notes: 'first intent' }
		});
		const send = vi.fn(async (mutations: Record<string, unknown>[]) => applied(mutations));
		await synchronizeCollection(1, send);
		expect(send.mock.calls).toHaveLength(2);
		expect(send.mock.calls[1][0][0]).toMatchObject({
			operation: 'update',
			entry_id: 1000,
			base_revision: 1,
			changes: { notes: 'later intent' }
		});
		expect((await listStoredCollectionEntries(1)).find((item) => item.id === 1000)).toMatchObject({
			notes: 'later intent',
			revision: 2,
			condition_grade: 'Z4'
		});
	});

	it('preserves an edit made while a request is in flight', async () => {
		await queueCollectionUpdate(1, entry.id, { notes: 'sent' });
		let finish!: (value: CollectionSyncResponse) => void;
		const send = vi
			.fn()
			.mockImplementationOnce(
				() =>
					new Promise<CollectionSyncResponse>((resolve) => {
						finish = resolve;
					})
			)
			.mockImplementation(async (mutations) => applied(mutations));
		const running = synchronizeCollection(1, send);
		await vi.waitFor(() => expect(send).toHaveBeenCalledOnce());
		await queueCollectionUpdate(1, entry.id, { notes: 'new intent' });
		const concurrent = synchronizeCollection(1, send);
		finish(applied(send.mock.calls[0][0]));
		await Promise.all([running, concurrent]);
		expect(send).toHaveBeenCalledTimes(2);
		expect((await listStoredCollectionEntries(1))[0]).toMatchObject({
			notes: 'new intent',
			revision: 3
		});
	});

	it('preserves optimistic data and its references across snapshot and page refreshes', async () => {
		await queueCollectionUpdate(1, entry.id, { notes: 'unsent' });
		await replaceOfflineSnapshot({ ...snapshot(1, []), series: [], issues: [] });
		await mergeCollectionEntries(1, [{ ...entry, notes: 'old server data' }]);
		expect((await listStoredCollectionEntries(1))[0]).toMatchObject({
			notes: 'unsent',
			sync_state: 'pending'
		});
		await expect(queueCollectionCreate(1, { issue_id: issue.id })).resolves.toHaveProperty('entry');
	});

	it('parks later edits with a conflict and explicitly reapplies their latest intent', async () => {
		await queueCollectionUpdate(1, entry.id, { notes: 'first' });
		await expect(
			synchronizeCollection(1, async () => {
				throw new TypeError('lost');
			})
		).rejects.toThrow();
		await queueCollectionUpdate(1, entry.id, { notes: 'latest' });
		await synchronizeCollection(1, async (mutations) => ({
			results: [
				{
					mutation_id: String(mutations[0].mutation_id),
					status: 'conflict',
					entry: { ...entry, notes: 'remote', revision: 3 },
					error: 'conflict',
					code: 'collection_revision_conflict'
				}
			]
		}));
		expect(await listMutations(1)).toEqual([]);
		const [conflict] = await listConflicts(1);
		await expect(queueCollectionUpdate(1, entry.id, { notes: 'oops' })).rejects.toThrow('Konflikt');
		await replaceOfflineSnapshot(snapshot(1));
		expect((await listStoredCollectionEntries(1))[0]).toMatchObject({
			notes: 'latest',
			sync_state: 'conflict'
		});
		await reapplyConflictLocalVersion(conflict);
		await expect(reapplyConflictLocalVersion(conflict)).rejects.toThrow('bereits');
		await synchronizeCollection(1, async (mutations) => applied(mutations));
		expect((await listStoredCollectionEntries(1))[0]).toMatchObject({
			notes: 'latest',
			revision: 4
		});
	});

	it('does not compete with another tab and takes over an expired lease', async () => {
		await queueCollectionUpdate(1, entry.id, { notes: 'pending' });
		const lease = (expires: number) =>
			editOfflineState(1, (state) =>
				state.metadata.set('sync-lease:1', {
					key: 'sync-lease:1',
					user_id: 1,
					value: JSON.stringify({ owner: 'other-tab', expires })
				})
			);
		await lease(Date.now() + 60_000);
		const send = vi.fn(async (mutations: Record<string, unknown>[]) => applied(mutations));
		expect(await synchronizeCollection(1, send)).toBeNull();
		expect(send).not.toHaveBeenCalled();
		await lease(0);
		await synchronizeCollection(1, send);
		expect(send).toHaveBeenCalledOnce();
	});

	it('keeps unacknowledged requests instead of looping on a partial response', async () => {
		await queueCollectionUpdate(1, entry.id, { notes: 'pending' });
		await expect(synchronizeCollection(1, async () => ({ results: [] }))).rejects.toThrow(
			'nicht alle'
		);
		expect(await listMutations(1)).toHaveLength(1);
	});
});
