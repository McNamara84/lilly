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

interface OfflineStatusLabelInput {
	online: boolean;
	syncing: boolean;
	pendingCount: number;
	conflictCount: number;
	syncError: string | null;
}

export function formatOfflineStatusLabel(status: OfflineStatusLabelInput): string {
	if (!status.online) {
		if (status.conflictCount > 0) return `Offline · ${status.conflictCount} Konflikt(e)`;
		if (status.pendingCount > 0) {
			return `Offline · ${status.pendingCount} Änderung(en) ausstehend`;
		}
		return 'Offline';
	}
	if (status.syncing) return 'Wird synchronisiert …';
	if (status.conflictCount > 0) return `${status.conflictCount} Konflikt(e)`;
	if (status.pendingCount > 0) return `${status.pendingCount} Änderung(en) ausstehend`;
	if (status.syncError) return 'Synchronisierung fehlgeschlagen';
	return 'Synchronisiert';
}

export function shouldProbeConnectivity(reportedOnline: boolean, serviceWorkerControlled: boolean) {
	return !reportedOnline || serviceWorkerControlled;
}

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

export async function refreshConnectivity(): Promise<boolean> {
	if (typeof navigator === 'undefined' || !navigator.onLine) {
		online = false;
		syncing = false;
		return false;
	}

	try {
		await fetch('/api/v1/health', {
			cache: 'no-store',
			credentials: 'same-origin'
		});
		online = true;
		return true;
	} catch {
		online = false;
		syncing = false;
		return false;
	}
}

export async function reconnectAndSynchronize(): Promise<void> {
	const wasOffline = !online;
	const reachable = await refreshConnectivity();
	if (wasOffline && reachable) await synchronizeNow();
}

export function initializeOfflineStatus(): void {
	if (initialized || typeof window === 'undefined') return;
	initialized = true;
	online = navigator.onLine;
	if (shouldProbeConnectivity(navigator.onLine, Boolean(navigator.serviceWorker?.controller))) {
		void refreshConnectivity();
	}
	void refreshOfflineStatus().catch(() => undefined);

	window.addEventListener('online', () => {
		void reconnectAndSynchronize();
	});
	window.addEventListener('offline', () => {
		online = false;
		syncing = false;
	});
	window.addEventListener('lilly:offline-change', () => {
		void refreshOfflineStatus().catch(() => undefined);
	});
	window.setInterval(() => {
		if (!online) void reconnectAndSynchronize();
	}, 5_000);

	if (typeof BroadcastChannel !== 'undefined') {
		const channel = new BroadcastChannel('lilly-offline');
		channel.addEventListener('message', () => {
			void refreshOfflineStatus().catch(() => undefined);
		});
	}
}
