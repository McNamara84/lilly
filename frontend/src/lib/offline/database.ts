import type { CollectionEntry } from '$lib/api/collection';
import type { MeResponse } from '$lib/api/auth';
import type { Issue, Series } from '$lib/api/series';
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

function deleteByUser(store: IDBObjectStore, userId: number): Promise<void> {
	return new Promise((resolve, reject) => {
		const request = store.index('user_id').openCursor(userId);
		request.addEventListener('error', () => reject(request.error), { once: true });
		request.addEventListener('success', () => {
			const cursor = request.result;
			if (!cursor) {
				resolve();
				return;
			}
			cursor.delete();
			cursor.continue();
		});
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
			database.addEventListener('versionchange', () => database.close());
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
	const database = await openDatabase();
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
	const database = await openDatabase();
	const transaction = database.transaction([STORES.profiles, STORES.metadata], 'readonly');
	const active = await requestResult<OfflineMetadata | undefined>(
		transaction.objectStore(STORES.metadata).get('active_user_id')
	);
	if (!active || typeof active.value !== 'number') return null;
	const stored = await requestResult<StoredProfile | undefined>(
		transaction.objectStore(STORES.profiles).get(active.value)
	);
	return stored?.profile ?? null;
}

export async function replaceOfflineSnapshot(snapshot: OfflineSnapshot): Promise<void> {
	const database = await openDatabase();
	const transaction = database.transaction(
		[STORES.series, STORES.issues, STORES.collection, STORES.metadata],
		'readwrite'
	);
	const done = transactionDone(transaction);
	await Promise.all([
		deleteByUser(transaction.objectStore(STORES.series), snapshot.user_id),
		deleteByUser(transaction.objectStore(STORES.issues), snapshot.user_id),
		deleteByUser(transaction.objectStore(STORES.collection), snapshot.user_id)
	]);

	const seriesStore = transaction.objectStore(STORES.series);
	for (const item of snapshot.series) {
		seriesStore.put({
			...item,
			key: userKey(snapshot.user_id, item.id),
			user_id: snapshot.user_id
		});
	}
	const issueStore = transaction.objectStore(STORES.issues);
	for (const item of snapshot.issues) {
		issueStore.put({ ...item, key: userKey(snapshot.user_id, item.id), user_id: snapshot.user_id });
	}
	const collectionStore = transaction.objectStore(STORES.collection);
	for (const item of snapshot.collection_entries) {
		collectionStore.put({
			...item,
			key: userKey(snapshot.user_id, item.id),
			user_id: snapshot.user_id
		});
	}
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
	if (entries.length === 0) return;
	const database = await openDatabase();
	const transaction = database.transaction(STORES.collection, 'readwrite');
	const done = transactionDone(transaction);
	const store = transaction.objectStore(STORES.collection);
	for (const entry of entries) {
		store.put({ ...entry, key: userKey(userId, entry.id), user_id: userId });
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
	const database = await openDatabase();
	const transaction = database.transaction(STORES.collection, 'readonly');
	const records = await requestResult<StoredCollectionEntry[]>(
		transaction.objectStore(STORES.collection).index('user_id').getAll(userId)
	);
	return records.map(({ key: _key, user_id: _userId, ...entry }) => entry);
}

export async function listStoredSeries(userId: number): Promise<Series[]> {
	const database = await openDatabase();
	const transaction = database.transaction(STORES.series, 'readonly');
	const records = await requestResult<StoredSeries[]>(
		transaction.objectStore(STORES.series).index('user_id').getAll(userId)
	);
	return records.map(({ key: _key, user_id: _userId, ...series }) => series);
}

export async function listStoredIssues(userId: number, seriesId?: number): Promise<Issue[]> {
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

export async function clearOfflineUserData(userId: number): Promise<void> {
	const database = await openDatabase();
	const transaction = database.transaction(Object.values(STORES), 'readwrite');
	const done = transactionDone(transaction);
	await Promise.all([
		deleteByUser(transaction.objectStore(STORES.series), userId),
		deleteByUser(transaction.objectStore(STORES.issues), userId),
		deleteByUser(transaction.objectStore(STORES.collection), userId),
		deleteByUser(transaction.objectStore(STORES.mutations), userId),
		deleteByUser(transaction.objectStore(STORES.conflicts), userId)
	]);
	transaction.objectStore(STORES.profiles).delete(userId);
	transaction.objectStore(STORES.metadata).delete(snapshotMetadataKey(userId));
	const active = await requestResult<OfflineMetadata | undefined>(
		transaction.objectStore(STORES.metadata).get('active_user_id')
	);
	if (active?.value === userId) transaction.objectStore(STORES.metadata).delete('active_user_id');
	await done;
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
