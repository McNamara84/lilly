import 'fake-indexeddb/auto';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import * as database from '#lib/offline/database.js';
import { changeOfflineSession } from '#lib/offline/session.js';
import { entry, issue, profile, snapshot } from './fixtures/offline';

// Interrupt the actual IndexedDB request before the application resumes its await.
function interruptRead(method: 'get' | 'getAll', interrupt: () => void, storeName?: string) {
	let interrupted = false;
	const attach = <T>(request: IDBRequest<T>, name: string) => {
		if (!interrupted && (!storeName || name === storeName)) {
			interrupted = true;
			request.addEventListener('success', interrupt, { once: true });
		}
		return request;
	};
	if (method === 'get') {
		const original = IDBObjectStore.prototype.get;
		vi.spyOn(IDBObjectStore.prototype, 'get').mockImplementation(function (
			this: IDBObjectStore,
			query
		) {
			return attach(original.call(this, query), this.name);
		});
	} else {
		const original = IDBIndex.prototype.getAll;
		vi.spyOn(IDBIndex.prototype, 'getAll').mockImplementation(function (this: IDBIndex, ...args) {
			return attach(original.apply(this, args), this.name);
		});
	}
}

describe('offline database session isolation', () => {
	beforeEach(async () => {
		localStorage.clear();
		await database.resetOfflineDatabaseForTests();
		changeOfflineSession(1, false);
		await database.saveConfirmedProfile(profile(1));
		await database.replaceOfflineSnapshot(snapshot(1));
	});

	afterEach(() => vi.restoreAllMocks());

	it('does not delete a newer login when a guarded cleanup resumes after its reads', async () => {
		const { offlineSessionVersion } = await import('#lib/offline/session.js');
		const version = offlineSessionVersion();
		interruptRead('get', () => changeOfflineSession(1, false), 'metadata');
		await database.clearOfflineUserData(1, version);
		expect(await database.getCachedProfile()).toEqual(profile(1));
		expect(await database.listStoredCollectionEntries(1)).toEqual([entry]);
	});

	it('does not restore a missing profile from a dangling active-user record', async () => {
		const connection = await new Promise<IDBDatabase>((resolve, reject) => {
			const request = indexedDB.open('lilly-offline');
			request.onsuccess = () => resolve(request.result);
			request.onerror = () => reject(request.error);
		});
		try {
			const transaction = connection.transaction('profiles', 'readwrite');
			transaction.objectStore('profiles').delete(1);
			await new Promise<void>((resolve, reject) => {
				transaction.oncomplete = () => resolve();
				transaction.onabort = () => reject(transaction.error);
			});
			expect(await database.getCachedProfile()).toBeNull();
		} finally {
			connection.close();
		}
	});

	it('hides every private store and ignores new writes for a logged-out account', async () => {
		changeOfflineSession(1, true);
		await database.saveConfirmedProfile(profile(1));
		await database.replaceOfflineSnapshot(snapshot(1, []));
		await database.mergeCollectionEntries(1, [{ ...entry, notes: 'stale response' }]);
		expect(await database.getCachedProfile()).toBeNull();
		expect(await database.listStoredCollectionEntries(1)).toEqual([]);
		expect(await database.listStoredSeries(1)).toEqual([]);
		expect(await database.listStoredIssues(1)).toEqual([]);
		expect(await database.getStoredIssue(1, issue.id)).toBeNull();
		expect(await database.listMutations(1)).toEqual([]);
		expect(await database.listConflicts(1)).toEqual([]);
		await expect(database.editOfflineState(1, () => undefined)).rejects.toThrow('Sitzung');
		changeOfflineSession(1, false);
		expect(await database.listStoredCollectionEntries(1)).toEqual([entry]);
	});

	it.each(['logout', 'blocked account'] as const)(
		'discards snapshots, page merges and queued edits interrupted by %s',
		async (reason) => {
			const interrupt = () => {
				if (reason === 'logout') changeOfflineSession(1, true);
				else localStorage.setItem('lilly-offline-session:blocked:1', 'true');
			};
			interruptRead('getAll', interrupt);
			await database.replaceOfflineSnapshot(snapshot(1, [{ ...entry, notes: 'late snapshot' }]));
			vi.restoreAllMocks();
			changeOfflineSession(1, false);
			expect(await database.listStoredCollectionEntries(1)).toEqual([entry]);

			interruptRead('get', interrupt);
			await database.mergeCollectionEntries(1, [{ ...entry, notes: 'late page' }]);
			vi.restoreAllMocks();
			changeOfflineSession(1, false);
			expect(await database.listStoredCollectionEntries(1)).toEqual([entry]);

			interruptRead('getAll', interrupt);
			const edit = vi.fn();
			await expect(database.editOfflineState(1, edit)).rejects.toThrow('Sitzung');
			expect(edit).not.toHaveBeenCalled();
			changeOfflineSession(1, false);
			expect(await database.listStoredCollectionEntries(1)).toEqual([entry]);
		}
	);

	it.each(['logout', 'blocked account'] as const)(
		'does not return a profile whose read was interrupted by %s',
		async (reason) => {
			interruptRead(
				'get',
				() => {
					if (reason === 'logout') changeOfflineSession(1, true);
					else localStorage.setItem('lilly-offline-session:blocked:1', 'true');
				},
				'profiles'
			);
			expect(await database.getCachedProfile()).toBeNull();
		}
	);

	it('leaves storage unchanged when merging an empty or identical collection page', async () => {
		const put = vi.spyOn(IDBObjectStore.prototype, 'put');
		await database.mergeCollectionEntries(1, []);
		await database.mergeCollectionEntries(1, [entry]);
		expect(put).not.toHaveBeenCalled();
		expect(await database.listStoredCollectionEntries(1)).toEqual([entry]);
	});
});
