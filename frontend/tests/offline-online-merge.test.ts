import 'fake-indexeddb/auto';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { fetchCollection } from '#lib/api/collection.js';
import * as database from '#lib/offline/database.js';
import { queueCollectionUpdate } from '#lib/offline/queue.js';
import { changeOfflineSession } from '#lib/offline/session.js';
import type { StoredConflict } from '#lib/offline/types.js';
import { entry, profile, snapshot } from './fixtures/offline';

function openOtherTab(): Promise<IDBDatabase> {
	return new Promise((resolve, reject) => {
		const request = indexedDB.open('lilly-offline', 1);
		request.addEventListener('success', () => resolve(request.result), { once: true });
		request.addEventListener('error', () => reject(request.error), { once: true });
	});
}

function completed(transaction: IDBTransaction): Promise<void> {
	return new Promise((resolve, reject) => {
		transaction.addEventListener('complete', () => resolve(), { once: true });
		transaction.addEventListener('abort', () => reject(transaction.error), { once: true });
	});
}

describe('atomic online collection merges', () => {
	beforeEach(async () => {
		localStorage.clear();
		await database.resetOfflineDatabaseForTests();
		changeOfflineSession(1, false);
		await database.saveConfirmedProfile(profile(1));
		await database.replaceOfflineSnapshot(snapshot(1));
	});

	afterEach(() => {
		vi.useRealTimers();
		vi.restoreAllMocks();
		vi.unstubAllGlobals();
	});

	it('merges a clean server page and reports no local changes', async () => {
		await expect(
			database.mergeOnlineCollectionEntries(1, [{ ...entry, notes: 'Server note' }])
		).resolves.toBe(false);
		expect((await database.listStoredCollectionEntries(1))[0].notes).toBe('Server note');
	});

	it.each(['none', 'before', 'after'] as const)(
		'lets a page interrupt and retry a snapshot (local edit: %s)',
		async (timing) => {
			const pending = timing === 'before';
			if (pending) await queueCollectionUpdate(1, entry.id, { notes: 'Pending edit' });
			vi.useFakeTimers({ toFake: ['setTimeout'] });
			const started = Promise.withResolvers<void>();
			const put = IDBObjectStore.prototype.put;
			let merge: Promise<boolean> | undefined;
			vi.spyOn(IDBObjectStore.prototype, 'put').mockImplementation(function (
				this: IDBObjectStore,
				value,
				key
			) {
				const request = put.call(this, value, key);
				if (
					!merge &&
					this.name === 'collection_entries' &&
					this.transaction.objectStoreNames.contains('metadata')
				) {
					merge = database.mergeOnlineCollectionEntries(1, [{ ...entry, notes: 'Online page' }]);
					started.resolve();
				}
				return request;
			});
			const refresh = database.replaceOfflineSnapshot(snapshot(1, [entry, { ...entry, id: 31 }]));
			await started.promise;
			await expect(merge).resolves.toBe(pending);
			const entries = await database.listStoredCollectionEntries(1);
			expect(entries).toHaveLength(1);
			expect(entries[0].notes).toBe(pending ? 'Pending edit' : 'Online page');
			if (timing === 'after') await queueCollectionUpdate(1, entry.id, { notes: 'Pending edit' });
			await vi.advanceTimersByTimeAsync(1000);
			await refresh;
			expect(await database.listStoredCollectionEntries(1)).toHaveLength(2);
			expect(await database.listMutations(1)).toHaveLength(timing === 'none' ? 0 : 1);
			if (timing !== 'none')
				expect((await database.listStoredCollectionEntries(1))[0].notes).toBe('Pending edit');
		}
	);

	it.each(['logout', 'new-login'] as const)(
		'does not retry an interrupted snapshot after %s',
		async (change) => {
			vi.useFakeTimers({ toFake: ['setTimeout'] });
			const getAll = IDBIndex.prototype.getAll;
			const started = Promise.withResolvers<void>();
			let merge: Promise<boolean> | undefined;
			vi.spyOn(IDBIndex.prototype, 'getAll').mockImplementationOnce(function (
				this: IDBIndex,
				...args
			) {
				const request = getAll.apply(this, args);
				merge = database.mergeOnlineCollectionEntries(1, [entry]);
				started.resolve();
				return request;
			});
			const refresh = database.replaceOfflineSnapshot(snapshot(1, [{ ...entry, id: 31 }]));
			await started.promise;
			await merge;
			changeOfflineSession(1, change === 'logout');
			await vi.advanceTimersByTimeAsync(1000);
			await refresh;
			if (change === 'logout') changeOfflineSession(1, false);
			expect(await database.listStoredCollectionEntries(1)).toEqual([entry]);
		}
	);

	it('still rejects an unexpected snapshot transaction failure', async () => {
		const getAll = IDBIndex.prototype.getAll;
		vi.spyOn(IDBIndex.prototype, 'getAll').mockImplementationOnce(function (
			this: IDBIndex,
			...args
		) {
			const request = getAll.apply(this, args);
			request.addEventListener('success', () => this.objectStore.transaction.abort(), {
				once: true
			});
			return request;
		});
		await expect(database.replaceOfflineSnapshot(snapshot(1))).rejects.toMatchObject({
			name: 'AbortError'
		});
	});

	it('uses local results for pending edits even when the server page is empty', async () => {
		await queueCollectionUpdate(1, entry.id, { notes: 'Pending edit' });
		vi.stubGlobal(
			'fetch',
			vi.fn().mockResolvedValue(Response.json({ data: [], total: 0, page: 1, per_page: 50 }))
		);
		const result = await fetchCollection();
		expect(result.total).toBe(1);
		expect(result.data[0].notes).toBe('Pending edit');
	});

	it('preserves a pending page entry even without a queue record or with an incoming sync flag', async () => {
		await database.mergeCollectionEntries(1, [
			{ ...entry, notes: 'Retained', sync_state: 'pending' }
		]);
		await expect(
			database.mergeOnlineCollectionEntries(1, [{ ...entry, sync_state: 'synced' }])
		).resolves.toBe(true);
		expect((await database.listStoredCollectionEntries(1))[0].notes).toBe('Retained');
	});

	it('ignores another account’s pending edits', async () => {
		await database.replaceOfflineSnapshot(snapshot(2, [{ ...entry, id: 31 }]));
		await queueCollectionUpdate(2, 31, { notes: 'Other account' });
		await expect(database.mergeOnlineCollectionEntries(1, [entry])).resolves.toBe(false);
		expect((await database.listStoredCollectionEntries(2))[0].notes).toBe('Other account');
	});

	it('rejects blocked sessions even for an empty page', async () => {
		changeOfflineSession(1, true);
		await expect(database.mergeOnlineCollectionEntries(1, [])).rejects.toThrow('Sitzung');
	});

	it('rolls back the merge if a page read aborts the transaction', async () => {
		const get = IDBObjectStore.prototype.get;
		vi.spyOn(IDBObjectStore.prototype, 'get').mockImplementationOnce(function (
			this: IDBObjectStore,
			key
		) {
			const request = get.call(this, key);
			this.transaction.abort();
			return request;
		});
		await expect(
			database.mergeOnlineCollectionEntries(1, [{ ...entry, notes: 'Aborted' }])
		).rejects.toMatchObject({ name: 'AbortError' });
		expect((await database.listStoredCollectionEntries(1))[0]).toEqual(entry);
	});

	it('rejects a session change during page reads before committing', async () => {
		const get = IDBObjectStore.prototype.get;
		vi.spyOn(IDBObjectStore.prototype, 'get').mockImplementationOnce(function (
			this: IDBObjectStore,
			key
		) {
			const request = get.call(this, key);
			request.addEventListener('success', () => changeOfflineSession(2, false), { once: true });
			return request;
		});
		await expect(
			database.mergeOnlineCollectionEntries(1, [{ ...entry, notes: 'Stale' }])
		).rejects.toThrow('Sitzung');
		expect((await database.listStoredCollectionEntries(1))[0]).toEqual(entry);
	});

	it('rejects a session change at transaction completion', async () => {
		const transaction = IDBDatabase.prototype.transaction;
		vi.spyOn(IDBDatabase.prototype, 'transaction').mockImplementation(function (
			this: IDBDatabase,
			stores,
			mode,
			options
		) {
			const current = transaction.call(this, stores, mode, options);
			if (current.objectStoreNames.contains('conflicts') && current.mode === 'readwrite') {
				current.addEventListener('complete', () => changeOfflineSession(2, false), {
					once: true
				});
			}
			return current;
		});
		await expect(database.mergeOnlineCollectionEntries(1, [entry])).rejects.toThrow('Sitzung');
	});

	it.each(['conflict-to-pending', 'new-edit-before-merge'] as const)(
		'shows the local edit when another connection performs %s',
		async (interleaving) => {
			await queueCollectionUpdate(1, entry.id, { notes: 'Pending edit' });
			const [mutation] = await database.listMutations(1);
			if (mutation.operation !== 'update') throw new Error('Expected an update');
			const conflict: StoredConflict = {
				mutation_id: mutation.mutation_id,
				user_id: 1,
				created_at: mutation.created_at,
				mutation,
				server_entry: { ...entry, revision: 2 },
				error: 'Conflict',
				code: 'revision_conflict',
				status: 'conflict'
			};
			await database.editOfflineState(1, (state) => {
				state.mutations.delete(mutation.mutation_id);
				state.entries.set(entry.id, {
					...entry,
					notes: interleaving === 'conflict-to-pending' ? 'Pending edit' : null,
					sync_state: interleaving === 'conflict-to-pending' ? 'conflict' : 'synced'
				});
				if (interleaving === 'conflict-to-pending')
					state.conflicts.set(conflict.mutation_id, conflict);
			});
			vi.stubGlobal(
				'fetch',
				vi.fn().mockResolvedValue(
					Response.json({
						data: [{ ...entry, notes: 'Server note' }],
						total: 1,
						page: 1,
						per_page: 50
					})
				)
			);
			const other = await openOtherTab();
			const transaction = IDBDatabase.prototype.transaction;
			let moved: Promise<void> | undefined;
			const move = () => {
				const writer = transaction.call(
					other,
					['collection_entries', 'mutations', 'conflicts'],
					'readwrite'
				);
				moved = completed(writer);
				writer
					.objectStore('mutations')
					.put({ ...mutation, mutation_id: crypto.randomUUID(), base_revision: 2 });
				writer.objectStore('conflicts').delete(conflict.mutation_id);
				writer.objectStore('collection_entries').put({
					...entry,
					key: `1:${entry.id}`,
					user_id: 1,
					notes: 'Pending edit',
					revision: 2,
					sync_state: 'pending'
				});
			};
			vi.spyOn(IDBDatabase.prototype, 'transaction').mockImplementation(function (
				this: IDBDatabase,
				stores,
				mode,
				options
			) {
				const names = typeof stores === 'string' ? [stores] : Array.from(stores);
				if (
					!moved &&
					interleaving === 'new-edit-before-merge' &&
					mode === 'readwrite' &&
					names.includes('collection_entries')
				)
					move();
				const current = transaction.call(this, stores, mode, options);
				if (!moved && interleaving === 'conflict-to-pending' && names.includes('mutations')) move();
				return current;
			});
			try {
				const result = await fetchCollection();
				expect(moved).toBeDefined();
				await moved;
				expect(result.data[0].notes).toBe('Pending edit');
				expect(result.data[0].sync_state).toBe('pending');
				expect((await database.listStoredCollectionEntries(1))[0].notes).toBe('Pending edit');
				expect(await database.listMutations(1)).toHaveLength(1);
				expect(await database.listConflicts(1)).toEqual([]);
			} finally {
				other.close();
			}
		}
	);
});
