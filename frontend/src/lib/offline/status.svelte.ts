import { syncPendingCollectionChanges } from '$lib/api/collection';
import { getCachedProfile, getSnapshotTimestamp } from './database';
import { listConflicts, listMutations } from './collection';

let online = $state(true);
let syncing = $state(false);
let pendingCount = $state(0);
let conflictCount = $state(0);
let lastSyncedAt = $state<string | null>(null);
let syncError = $state<string | null>(null);
let initialized = false;

export function getOfflineStatus() {
	return {
		get online() {
			return online;
		},
		get syncing() {
			return syncing;
		},
		get pendingCount() {
			return pendingCount;
		},
		get conflictCount() {
			return conflictCount;
		},
		get lastSyncedAt() {
			return lastSyncedAt;
		},
		get syncError() {
			return syncError;
		}
	};
}

export async function refreshOfflineStatus(): Promise<void> {
	const profile = await getCachedProfile().catch(() => null);
	if (!profile) {
		pendingCount = 0;
		conflictCount = 0;
		lastSyncedAt = null;
		return;
	}
	const [mutations, conflicts, timestamp] = await Promise.all([
		listMutations(profile.id),
		listConflicts(profile.id),
		getSnapshotTimestamp(profile.id)
	]);
	pendingCount = mutations.length;
	conflictCount = conflicts.length;
	lastSyncedAt = timestamp;
}

export async function synchronizeNow(): Promise<void> {
	if (!online || syncing) return;
	syncing = true;
	syncError = null;
	try {
		await syncPendingCollectionChanges();
	} catch (error) {
		syncError = error instanceof Error ? error.message : 'Synchronisierung fehlgeschlagen';
	} finally {
		syncing = false;
		await refreshOfflineStatus().catch(() => undefined);
	}
}

export function initializeOfflineStatus(): void {
	if (initialized || typeof window === 'undefined') return;
	initialized = true;
	online = navigator.onLine;
	void refreshOfflineStatus().catch(() => undefined);

	window.addEventListener('online', () => {
		online = true;
		void synchronizeNow();
	});
	window.addEventListener('offline', () => {
		online = false;
		syncing = false;
	});
	window.addEventListener('lilly:offline-change', () => {
		void refreshOfflineStatus().catch(() => undefined);
	});

	if (typeof BroadcastChannel !== 'undefined') {
		const channel = new BroadcastChannel('lilly-offline');
		channel.addEventListener('message', () => {
			void refreshOfflineStatus().catch(() => undefined);
		});
	}
}
