import 'fake-indexeddb/auto';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import * as database from '#lib/offline/database.js';
import {
	acceptConflictServerVersion,
	queueCollectionCreate,
	queueCollectionUpdate,
	synchronizeCollection
} from '#lib/offline/queue.js';
import { changeOfflineSession } from '#lib/offline/session.js';
import type { CollectionSyncResponse } from '#lib/offline/types.js';
import { entry, issue, snapshot } from './fixtures/offline';

async function lease() {
	return database.editOfflineState(1, (state) => {
		const value = state.metadata.get('sync-lease:1')?.value;
		return value ? (JSON.parse(String(value)) as { owner: string; expires: number }) : null;
	});
}

async function takeOver() {
	await database.editOfflineState(1, (state) => {
		state.metadata.set('sync-lease:1', {
			key: 'sync-lease:1',
			user_id: 1,
			value: JSON.stringify({ owner: 'another-tab', expires: Date.now() + 60_000 })
		});
	});
}

describe('queue ownership and interrupted synchronization', () => {
	beforeEach(async () => {
		localStorage.clear();
		await database.resetOfflineDatabaseForTests();
		changeOfflineSession(1, false);
		await database.replaceOfflineSnapshot(snapshot(1));
	});
	afterEach(() => {
		vi.useRealTimers();
		vi.restoreAllMocks();
		vi.unstubAllGlobals();
	});

	it('stops before sending if another tab takes ownership between claiming and selecting work', async () => {
		await queueCollectionUpdate(1, entry.id, { notes: 'pending' });
		const original = database.editOfflineState;
		let first = true;
		const intercept: typeof original = async (userId, edit) => {
			const result = await original(userId, edit);
			if (first) {
				first = false;
				await takeOver();
			}
			return result;
		};
		vi.spyOn(database, 'editOfflineState').mockImplementation(intercept);
		const send = vi.fn();
		expect(await synchronizeCollection(1, send)).toEqual({ results: [] });
		expect(send).not.toHaveBeenCalled();
		expect(await lease()).toMatchObject({ owner: 'another-tab' });
		expect((await database.listMutations(1))[0].attempts).toBe(0);
	});

	it.each([false, true])(
		'drains newly queued work for a waiting caller after an earlier pass (had work: %s)',
		async (hadWork) => {
			if (hadWork) await queueCollectionUpdate(1, entry.id, { notes: 'first' });
			const original = database.editOfflineState;
			let added = false;
			const intercept: typeof original = async (userId, edit) => {
				let released = false;
				const result = await original(userId, (state) => {
					const claimed = state.metadata.has('sync-lease:1');
					const result = edit(state);
					released = claimed && !state.metadata.has('sync-lease:1');
					return result;
				});
				if (!added && (released || (!hadWork && result === false))) {
					added = true;
					await queueCollectionUpdate(1, entry.id, { notes: 'queued during completion' });
				}
				return result;
			};
			vi.spyOn(database, 'editOfflineState').mockImplementation(intercept);
			const send = vi.fn(async (mutations: Record<string, unknown>[]) => ({
				results: mutations.map((mutation) => ({
					mutation_id: String(mutation.mutation_id),
					status: 'applied' as const,
					entry: {
						...entry,
						...(mutation.changes as object),
						revision: Number(mutation.base_revision) + 1
					},
					error: null,
					code: null
				}))
			}));
			const first = synchronizeCollection(1, send);
			const waiting = synchronizeCollection(1, send);
			const [, combined] = await Promise.all([first, waiting]);
			expect(combined!.results).toHaveLength(hadWork ? 2 : 1);
			expect(send).toHaveBeenCalledTimes(hadWork ? 2 : 1);
			expect(await database.listMutations(1)).toEqual([]);
			expect((await database.listStoredCollectionEntries(1))[0].notes).toBe(
				'queued during completion'
			);
		}
	);

	it('does not recreate a mutation resolved by another tab when its original request fails', async () => {
		const queued = await queueCollectionUpdate(1, entry.id, { notes: 'pending' });
		await expect(
			synchronizeCollection(1, async () => {
				await database.deleteMutation(queued.mutation.mutation_id);
				throw new Error('lost response');
			})
		).rejects.toThrow('lost response');
		expect(await database.listMutations(1)).toEqual([]);
		expect(await lease()).toBeNull();
	});

	it('replays persisted dependent edits in order and only rebases the direct successor', async () => {
		const first = await queueCollectionUpdate(1, entry.id, { notes: 'first' });
		if (first.mutation.operation !== 'update') throw new Error('Expected an update');
		await database.enqueueMutation({
			...first.mutation,
			mutation_id: 'second',
			depends_on: first.mutation.mutation_id,
			changes: { notes: 'second' },
			created_at: '2099-01-01T00:00:00Z'
		});
		await database.enqueueMutation({
			...first.mutation,
			mutation_id: 'third',
			depends_on: 'second',
			changes: { notes: 'third' },
			created_at: '2099-01-02T00:00:00Z'
		});
		const send = vi.fn(async (mutations: Record<string, unknown>[]) => ({
			results: mutations.map((mutation) => ({
				mutation_id: String(mutation.mutation_id),
				status: 'applied' as const,
				entry: {
					...entry,
					...(mutation.changes as object),
					revision:
						mutation.mutation_id === first.mutation.mutation_id
							? null
							: Number(mutation.base_revision) + 1
				},
				error: null,
				code: null
			}))
		}));
		await synchronizeCollection(1, send);
		expect(
			send.mock.calls.map(([batch]) =>
				batch.map((mutation) => [mutation.mutation_id, mutation.base_revision])
			)
		).toEqual([[[first.mutation.mutation_id, 1]], [['second', 0]], [['third', 1]]]);
		expect(await database.listMutations(1)).toEqual([]);
		expect((await database.listStoredCollectionEntries(1))[0]).toMatchObject({
			notes: 'third',
			revision: 2
		});
	});

	it('renews ownership while a request is slow, then stops renewing after another tab takes over', async () => {
		vi.useFakeTimers({ toFake: ['Date', 'setInterval', 'clearInterval'] });
		const queued = await queueCollectionUpdate(1, entry.id, { notes: 'pending' });
		const response = Promise.withResolvers<CollectionSyncResponse>();
		const send = vi.fn(() => response.promise);
		const running = synchronizeCollection(1, send);
		try {
			await vi.waitFor(() => expect(send).toHaveBeenCalledOnce());
			const initial = await lease();
			await vi.advanceTimersByTimeAsync(20_000);
			await vi.waitFor(async () =>
				expect((await lease())!.expires).toBeGreaterThan(initial!.expires)
			);
			await takeOver();
			const successor = await lease();
			await vi.advanceTimersByTimeAsync(20_000);
			expect(await lease()).toEqual(successor);
		} finally {
			response.resolve({
				results: [
					{
						mutation_id: queued.mutation.mutation_id,
						status: 'applied',
						entry: { ...entry, revision: 2 },
						error: null,
						code: null
					}
				]
			});
			await running;
		}
		expect(await lease()).toMatchObject({ owner: 'another-tab' });
		expect(await database.listMutations(1)).toHaveLength(1);
		expect((await database.listStoredCollectionEntries(1))[0]).toMatchObject({
			notes: 'pending',
			revision: 1
		});
		expect(vi.getTimerCount()).toBe(0);
	});

	it.each(['removed', 'taken over'] as const)(
		'does not record a request failure after its lease was %s',
		async (state) => {
			await queueCollectionUpdate(1, entry.id, { notes: 'pending' });
			const send = async () => {
				if (state === 'taken over') await takeOver();
				else await database.editOfflineState(1, (data) => data.metadata.delete('sync-lease:1'));
				throw new TypeError('old request failed');
			};
			await expect(synchronizeCollection(1, send)).rejects.toThrow('old request failed');
			expect((await database.listMutations(1))[0].last_error).toBeNull();
			if (state === 'taken over') expect(await lease()).toMatchObject({ owner: 'another-tab' });
			else expect(await lease()).toBeNull();
		}
	);

	it('handles heartbeat and cleanup rejection after logout without recreating private data', async () => {
		vi.useFakeTimers({ toFake: ['Date', 'setInterval', 'clearInterval'] });
		await queueCollectionUpdate(1, entry.id, { notes: 'private' });
		const response = Promise.withResolvers<CollectionSyncResponse>();
		const send = vi.fn(() => response.promise);
		const running = synchronizeCollection(1, send);
		const failed = expect(running).rejects.toThrow('request failed after logout');
		await vi.waitFor(() => expect(send).toHaveBeenCalledOnce());
		changeOfflineSession(1, true);
		await database.clearOfflineUserData(1);
		await vi.advanceTimersByTimeAsync(20_000);
		response.reject(new Error('request failed after logout'));
		await failed;
		changeOfflineSession(1, false);
		expect(await database.listStoredCollectionEntries(1)).toEqual([]);
		expect(await database.listMutations(1)).toEqual([]);
		expect(await lease()).toBeNull();
		expect(vi.getTimerCount()).toBe(0);
	});

	it('preserves the latest edit when a create conflicts, and accepts the durable decision once', async () => {
		const created = await queueCollectionCreate(1, { issue_id: issue.id, notes: 'first' });
		await expect(
			synchronizeCollection(1, async () => {
				throw new TypeError('offline');
			})
		).rejects.toThrow('offline');
		await queueCollectionUpdate(1, created.entry.id, { notes: 'latest' });
		await synchronizeCollection(1, async () => ({
			results: [
				{
					mutation_id: created.mutation.mutation_id,
					status: 'rejected',
					entry: null,
					error: null,
					code: null
				}
			]
		}));
		const [conflict] = await database.listConflicts(1);
		expect(conflict.mutation).toMatchObject({ operation: 'create', entry: { notes: 'latest' } });
		expect(await database.listMutations(1)).toEqual([]);
		await acceptConflictServerVersion(conflict);
		await acceptConflictServerVersion(conflict);
		expect(await database.listConflicts(1)).toEqual([]);
		expect(await database.listStoredCollectionEntries(1)).toEqual([entry]);
	});

	it('normalizes cleared optional text and broadcasts changes only when the APIs are available', async () => {
		vi.stubGlobal('BroadcastChannel', undefined);
		vi.stubGlobal('window', undefined);
		await queueCollectionUpdate(1, entry.id, { notes: '   ', edition_label: '  ' });
		expect((await database.listStoredCollectionEntries(1))[0]).toMatchObject({
			notes: null,
			edition_label: null,
			sync_state: 'pending'
		});
	});

	it('orders legacy equal timestamps deterministically by mutation id', async () => {
		const first = await queueCollectionCreate(1, { issue_id: issue.id });
		const second = await queueCollectionCreate(1, { issue_id: issue.id });
		await database.editOfflineState(1, (state) => {
			for (const mutation of state.mutations.values())
				mutation.created_at = first.mutation.created_at;
		});
		const send = vi.fn(async (mutations: Record<string, unknown>[]) => ({
			results: mutations.map((mutation) => ({
				mutation_id: String(mutation.mutation_id),
				status: 'already_applied' as const,
				entry: null,
				error: null,
				code: null
			}))
		}));
		await synchronizeCollection(1, send);
		expect(send.mock.calls[0][0].map((item) => item.mutation_id)).toEqual(
			[first.mutation.mutation_id, second.mutation.mutation_id].sort()
		);
		expect(await database.listMutations(1)).toEqual([]);
	});
});
