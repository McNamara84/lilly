import type { CollectionEntry } from '#lib/api/collection.js';
import type { MeResponse } from '#lib/api/auth.js';
import type { Issue, Series } from '#lib/api/series.js';
import { isOfflineUserBlocked, offlineSessionVersion, isCurrentOfflineSession } from './session';
import type {
	CollectionMutation,
	OfflineMetadata,
	OfflineSnapshot,
	StoredCollectionEntry,
	StoredConflict,
	StoredIssue,
	StoredProfile,
	StoredSeries
} from './types';

const DATABASE_NAME = 'lilly-offline';
const DATABASE_VERSION = 1;

const STORES = {
	profiles: 'profiles',
	series: 'series',
	issues: 'issues',
	collection: 'collection_entries',
	mutations: 'mutations',
	conflicts: 'conflicts',
	metadata: 'metadata'
} as const;

let databasePromise: Promise<IDBDatabase> | null = null;

function requestResult<T>(request: IDBRequest<T>): Promise<T> {
	return new Promise((resolve, reject) => {
		request.addEventListener('success', () => resolve(request.result), { once: true });
		request.addEventListener('error', () => reject(request.error), { once: true });
	});
}

function transactionDone(transaction: IDBTransaction): Promise<void> {
	return new Promise((resolve, reject) => {
		transaction.addEventListener('complete', () => resolve(), { once: true });
		transaction.addEventListener('abort', () => reject(transaction.error), { once: true });
		transaction.addEventListener('error', () => reject(transaction.error), { once: true });
	});
}

function createUserStore(database: IDBDatabase, name: string, keyPath: string): IDBObjectStore {
	const store = database.createObjectStore(name, { keyPath });
	store.createIndex('user_id', 'user_id', { unique: false });
	return store;
}

function openDatabase(): Promise<IDBDatabase> {
	if (databasePromise) return databasePromise;
	if (typeof indexedDB === 'undefined') {
		return Promise.reject(new Error('IndexedDB is not available'));
	}

	databasePromise = new Promise((resolve, reject) => {
		const request = indexedDB.open(DATABASE_NAME, DATABASE_VERSION);
		request.addEventListener('upgradeneeded', () => {
			const database = request.result;
			createUserStore(database, STORES.profiles, 'user_id');
			createUserStore(database, STORES.series, 'key');
			createUserStore(database, STORES.issues, 'key');
			createUserStore(database, STORES.collection, 'key');
			createUserStore(database, STORES.mutations, 'mutation_id');
			createUserStore(database, STORES.conflicts, 'mutation_id');
			createUserStore(database, STORES.metadata, 'key');
		});
		request.addEventListener('success', () => {
			const database = request.result;
			database.addEventListener('versionchange', () => {
				database.close();
				databasePromise = null;
			});
			resolve(database);
		});
		request.addEventListener('error', () => {
			databasePromise = null;
			reject(request.error);
		});
	});
	return databasePromise;
}

function userKey(userId: number, entityId: number): string {
	return `${userId}:${entityId}`;
}

function snapshotMetadataKey(userId: number): string {
	return `snapshot:${userId}`;
}

export async function saveConfirmedProfile(profile: MeResponse): Promise<void> {
	const version = offlineSessionVersion();
	if (isOfflineUserBlocked(profile.id)) return;
	const database = await openDatabase();
	if (!isCurrentOfflineSession(version) || isOfflineUserBlocked(profile.id)) return;
	const transaction = database.transaction([STORES.profiles, STORES.metadata], 'readwrite');
	const done = transactionDone(transaction);
	const stored: StoredProfile = {
		user_id: profile.id,
		profile,
		confirmed_at: new Date().toISOString()
	};
	transaction.objectStore(STORES.profiles).put(stored);
	transaction.objectStore(STORES.metadata).put({
		key: 'active_user_id',
		user_id: profile.id,
		value: profile.id
	} satisfies OfflineMetadata);
	await done;
}

export async function getCachedProfile(): Promise<MeResponse | null> {
	const version = offlineSessionVersion();
	const database = await openDatabase();
	const transaction = database.transaction([STORES.profiles, STORES.metadata], 'readonly');
	const active = await requestResult<OfflineMetadata | undefined>(
		transaction.objectStore(STORES.metadata).get('active_user_id')
	);
	if (!active || typeof active.value !== 'number') return null;
	if (isOfflineUserBlocked(active.value)) return null;
	const stored = await requestResult<StoredProfile | undefined>(
		transaction.objectStore(STORES.profiles).get(active.value)
	);
	return isCurrentOfflineSession(version) && !isOfflineUserBlocked(active.value)
		? (stored?.profile ?? null)
		: null;
}

export async function replaceOfflineSnapshot(snapshot: OfflineSnapshot): Promise<void> {
	const version = offlineSessionVersion();
	if (isOfflineUserBlocked(snapshot.user_id)) return;
	const database = await openDatabase();
	const transaction = database.transaction(
		[STORES.series, STORES.issues, STORES.collection, STORES.metadata],
		'readwrite'
	);
	const done = transactionDone(transaction);
	const [localSeries, localIssues, localEntries] = await Promise.all([
		requestResult<StoredSeries[]>(
			transaction.objectStore(STORES.series).index('user_id').getAll(snapshot.user_id)
		),
		requestResult<StoredIssue[]>(
			transaction.objectStore(STORES.issues).index('user_id').getAll(snapshot.user_id)
		),
		requestResult<StoredCollectionEntry[]>(
			transaction.objectStore(STORES.collection).index('user_id').getAll(snapshot.user_id)
		)
	]);
	const retained = localEntries.filter(
		(entry) => entry.sync_state === 'pending' || entry.sync_state === 'conflict'
	);
	const retainedIssueIds = new Set(retained.map((entry) => entry.issue_id));
	const retainedSeriesIds = new Set(retained.map((entry) => entry.series_id));
	if (!isCurrentOfflineSession(version) || isOfflineUserBlocked(snapshot.user_id)) {
		await done;
		return;
	}
	// Avoid rewriting the whole catalog on every navigation. In WebKit, hundreds of
	// redundant writes hold the transaction lock long enough to block interactive reads.
	const reconcile = <T extends { id: number }>(
		storeName: string,
		before: (T & { key: string; user_id: number })[],
		after: T[]
	) => {
		const store = transaction.objectStore(storeName);
		const previous = new Map(before.map((item) => [item.id, item]));
		for (const item of after) {
			const stored = {
				...item,
				key: userKey(snapshot.user_id, item.id),
				user_id: snapshot.user_id
			};
			if (JSON.stringify(previous.get(item.id)) !== JSON.stringify(stored)) store.put(stored);
			previous.delete(item.id);
		}
		for (const item of previous.values()) store.delete(item.key);
	};
	// Pending entries override the server view; retain their catalog references too.
	reconcile(STORES.series, localSeries, [
		...snapshot.series,
		...localSeries.filter(
			(item) =>
				retainedSeriesIds.has(item.id) && !snapshot.series.some((series) => series.id === item.id)
		)
	]);
	reconcile(STORES.issues, localIssues, [
		...snapshot.issues,
		...localIssues.filter(
			(item) =>
				retainedIssueIds.has(item.id) && !snapshot.issues.some((issue) => issue.id === item.id)
		)
	]);
	const pendingIds = new Set(retained.map((entry) => entry.id));
	reconcile(STORES.collection, localEntries, [
		...snapshot.collection_entries.filter((entry) => !pendingIds.has(entry.id)),
		...retained
	]);
	transaction.objectStore(STORES.metadata).put({
		key: snapshotMetadataKey(snapshot.user_id),
		user_id: snapshot.user_id,
		value: JSON.stringify({
			schema_version: snapshot.schema_version,
			snapshot_version: snapshot.snapshot_version,
			generated_at: snapshot.generated_at
		})
	} satisfies OfflineMetadata);
	await done;
}

export async function mergeCollectionEntries(
	userId: number,
	entries: CollectionEntry[]
): Promise<void> {
	const version = offlineSessionVersion();
	if (entries.length === 0 || isOfflineUserBlocked(userId)) return;
	const database = await openDatabase();
	const transaction = database.transaction(STORES.collection, 'readwrite');
	const done = transactionDone(transaction);
	const store = transaction.objectStore(STORES.collection);
	const existing = await Promise.all(
		entries.map((entry) =>
			requestResult<StoredCollectionEntry | undefined>(store.get(userKey(userId, entry.id)))
		)
	);
	for (const [index, entry] of entries.entries()) {
		const current = existing[index];
		if (!isCurrentOfflineSession(version) || isOfflineUserBlocked(userId)) break;
		if (
			!entry.sync_state &&
			(current?.sync_state === 'pending' || current?.sync_state === 'conflict')
		)
			continue;
		const stored = { ...entry, key: userKey(userId, entry.id), user_id: userId };
		if (JSON.stringify(current) !== JSON.stringify(stored)) store.put(stored);
	}
	await done;
}

export async function putCollectionEntry(userId: number, entry: CollectionEntry): Promise<void> {
	await mergeCollectionEntries(userId, [entry]);
}

export async function deleteStoredCollectionEntry(userId: number, entryId: number): Promise<void> {
	const database = await openDatabase();
	const transaction = database.transaction(STORES.collection, 'readwrite');
	const done = transactionDone(transaction);
	transaction.objectStore(STORES.collection).delete(userKey(userId, entryId));
	await done;
}

export async function listStoredCollectionEntries(userId: number): Promise<CollectionEntry[]> {
	if (isOfflineUserBlocked(userId)) return [];
	const database = await openDatabase();
	const transaction = database.transaction(STORES.collection, 'readonly');
	const records = await requestResult<StoredCollectionEntry[]>(
		transaction.objectStore(STORES.collection).index('user_id').getAll(userId)
	);
	return records.map(({ key: _key, user_id: _userId, ...entry }) => entry);
}

export async function listStoredSeries(userId: number): Promise<Series[]> {
	if (isOfflineUserBlocked(userId)) return [];
	const database = await openDatabase();
	const transaction = database.transaction(STORES.series, 'readonly');
	const records = await requestResult<StoredSeries[]>(
		transaction.objectStore(STORES.series).index('user_id').getAll(userId)
	);
	return records.map(({ key: _key, user_id: _userId, ...series }) => series);
}

export async function listStoredIssues(userId: number, seriesId?: number): Promise<Issue[]> {
	if (isOfflineUserBlocked(userId)) return [];
	const database = await openDatabase();
	const transaction = database.transaction(STORES.issues, 'readonly');
	const records = await requestResult<StoredIssue[]>(
		transaction.objectStore(STORES.issues).index('user_id').getAll(userId)
	);
	return records
		.filter((issue) => seriesId === undefined || issue.series_id === seriesId)
		.map(({ key: _key, user_id: _userId, ...issue }) => issue);
}

export async function getStoredIssue(userId: number, issueId: number): Promise<Issue | null> {
	if (isOfflineUserBlocked(userId)) return null;
	const database = await openDatabase();
	const transaction = database.transaction(STORES.issues, 'readonly');
	const record = await requestResult<StoredIssue | undefined>(
		transaction.objectStore(STORES.issues).get(userKey(userId, issueId))
	);
	if (!record) return null;
	const { key: _key, user_id: _userId, ...issue } = record;
	return issue;
}

export async function enqueueMutation(mutation: CollectionMutation): Promise<void> {
	const database = await openDatabase();
	const transaction = database.transaction(STORES.mutations, 'readwrite');
	const done = transactionDone(transaction);
	transaction.objectStore(STORES.mutations).put(mutation);
	await done;
}

export async function listMutations(userId: number): Promise<CollectionMutation[]> {
	if (isOfflineUserBlocked(userId)) return [];
	const database = await openDatabase();
	const transaction = database.transaction(STORES.mutations, 'readonly');
	const mutations = await requestResult<CollectionMutation[]>(
		transaction.objectStore(STORES.mutations).index('user_id').getAll(userId)
	);
	return mutations.sort((left, right) => left.created_at.localeCompare(right.created_at));
}

export async function deleteMutation(mutationId: string): Promise<void> {
	const database = await openDatabase();
	const transaction = database.transaction(STORES.mutations, 'readwrite');
	const done = transactionDone(transaction);
	transaction.objectStore(STORES.mutations).delete(mutationId);
	await done;
}

export async function putConflict(conflict: StoredConflict): Promise<void> {
	const database = await openDatabase();
	const transaction = database.transaction(STORES.conflicts, 'readwrite');
	const done = transactionDone(transaction);
	transaction.objectStore(STORES.conflicts).put(conflict);
	await done;
}

export async function listConflicts(userId: number): Promise<StoredConflict[]> {
	if (isOfflineUserBlocked(userId)) return [];
	const database = await openDatabase();
	const transaction = database.transaction(STORES.conflicts, 'readonly');
	return requestResult<StoredConflict[]>(
		transaction.objectStore(STORES.conflicts).index('user_id').getAll(userId)
	);
}

export async function deleteConflict(mutationId: string): Promise<void> {
	const database = await openDatabase();
	const transaction = database.transaction(STORES.conflicts, 'readwrite');
	const done = transactionDone(transaction);
	transaction.objectStore(STORES.conflicts).delete(mutationId);
	await done;
}

export async function getSnapshotTimestamp(userId: number): Promise<string | null> {
	const database = await openDatabase();
	const transaction = database.transaction(STORES.metadata, 'readonly');
	const metadata = await requestResult<OfflineMetadata | undefined>(
		transaction.objectStore(STORES.metadata).get(snapshotMetadataKey(userId))
	);
	if (!metadata || typeof metadata.value !== 'string') return null;
	try {
		return (JSON.parse(metadata.value) as { generated_at?: string }).generated_at ?? null;
	} catch {
		return null;
	}
}

export async function clearOfflineUserData(userId: number, version?: string): Promise<void> {
	const database = await openDatabase();
	const transaction = database.transaction(Object.values(STORES), 'readwrite');
	const done = transactionDone(transaction);
	const stores = [
		STORES.series,
		STORES.issues,
		STORES.collection,
		STORES.mutations,
		STORES.conflicts,
		STORES.metadata
	];
	const [keys, active] = await Promise.all([
		Promise.all(
			stores.map((name) =>
				requestResult<IDBValidKey[]>(
					transaction.objectStore(name).index('user_id').getAllKeys(userId)
				)
			)
		),
		requestResult<OfflineMetadata | undefined>(
			transaction.objectStore(STORES.metadata).get('active_user_id')
		)
	]);
	// A newer login can arrive while these reads wait for another tab's transaction.
	if (version === undefined || isCurrentOfflineSession(version)) {
		for (const [index, storeKeys] of keys.entries()) {
			const store = transaction.objectStore(stores[index]);
			for (const key of storeKeys) store.delete(key);
		}
		transaction.objectStore(STORES.profiles).delete(userId);
		if (active?.value === userId) transaction.objectStore(STORES.metadata).delete('active_user_id');
	}
	await done;
}

/** Resume cleanup interrupted by a closed/navigated tab before accepting another session. */
export async function clearBlockedOfflineUsers(): Promise<void> {
	const database = await openDatabase();
	const transaction = database.transaction(STORES.profiles, 'readonly');
	const profiles = await requestResult<StoredProfile[]>(
		transaction.objectStore(STORES.profiles).getAll()
	);
	for (const profile of profiles) {
		const version = offlineSessionVersion();
		if (isOfflineUserBlocked(profile.user_id)) await clearOfflineUserData(profile.user_id, version);
	}
}

export async function resetOfflineDatabaseForTests(): Promise<void> {
	if (databasePromise) {
		const database = await databasePromise;
		database.close();
		databasePromise = null;
	}
	if (typeof indexedDB === 'undefined') return;
	await requestResult(indexedDB.deleteDatabase(DATABASE_NAME));
}

export interface OfflineWriteState {
	entries: Map<number, CollectionEntry>;
	mutations: Map<string, CollectionMutation>;
	conflicts: Map<string, StoredConflict>;
	metadata: Map<string, OfflineMetadata>;
}

/** Serialize read/modify/write across tabs. The callback must remain synchronous. */
export async function editOfflineState<T>(
	userId: number,
	edit: (state: OfflineWriteState) => T
): Promise<T> {
	const version = offlineSessionVersion();
	const database = await openDatabase();
	const transaction = database.transaction(
		[STORES.collection, STORES.mutations, STORES.conflicts, STORES.metadata],
		'readwrite'
	);
	const done = transactionDone(transaction);
	try {
		const names = [STORES.collection, STORES.mutations, STORES.conflicts, STORES.metadata];
		const rows = await Promise.all(
			names.map((name) =>
				requestResult<unknown[]>(transaction.objectStore(name).index('user_id').getAll(userId))
			)
		);
		const originals = rows.map((items, index) => {
			const key = transaction.objectStore(names[index]).keyPath as string;
			return new Map(
				(items as Record<string, unknown>[]).map((item) => [item[key], JSON.stringify(item)])
			);
		});
		if (!isCurrentOfflineSession(version) || isOfflineUserBlocked(userId))
			throw new Error('Die lokale Sitzung wurde beendet.');
		const state: OfflineWriteState = {
			entries: new Map(
				(rows[0] as StoredCollectionEntry[]).map(({ key: _key, user_id: _userId, ...entry }) => [
					entry.id,
					entry
				])
			),
			mutations: new Map((rows[1] as CollectionMutation[]).map((item) => [item.mutation_id, item])),
			conflicts: new Map((rows[2] as StoredConflict[]).map((item) => [item.mutation_id, item])),
			metadata: new Map((rows[3] as OfflineMetadata[]).map((item) => [item.key, item]))
		};
		const result = edit(state);
		const values = [
			[...state.entries.values()].map((entry) => ({
				...entry,
				key: userKey(userId, entry.id),
				user_id: userId
			})),
			[...state.mutations.values()],
			[...state.conflicts.values()],
			[...state.metadata.values()]
		];
		for (let index = 0; index < names.length; index++) {
			const store = transaction.objectStore(names[index]);
			const key = store.keyPath as string;
			const before = originals[index];
			for (const item of values[index]) {
				const id = (item as unknown as Record<string, unknown>)[key];
				if (before.get(id) !== JSON.stringify(item)) store.put(item);
				before.delete(id);
			}
			for (const id of before.keys()) store.delete(id as IDBValidKey);
		}
		await done;
		return result;
	} catch (error) {
		try {
			transaction.abort();
		} catch {
			/* The transaction may already have aborted. */
		}
		await done.catch(() => undefined);
		throw error;
	}
}
