import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
	syncPendingCollectionChanges: vi.fn(),
	getCachedProfile: vi.fn(),
	getSnapshotTimestamp: vi.fn(),
	listMutations: vi.fn(),
	listConflicts: vi.fn()
}));

vi.mock('$lib/api/collection', () => ({
	syncPendingCollectionChanges: mocks.syncPendingCollectionChanges
}));
vi.mock('$lib/offline/database', () => ({
	getCachedProfile: mocks.getCachedProfile,
	getSnapshotTimestamp: mocks.getSnapshotTimestamp
}));
vi.mock('$lib/offline/collection', () => ({
	listMutations: mocks.listMutations,
	listConflicts: mocks.listConflicts
}));

describe('offline status lifecycle', () => {
	beforeEach(() => {
		vi.resetModules();
		vi.clearAllMocks();
		mocks.getCachedProfile.mockResolvedValue({ id: 7 });
		mocks.getSnapshotTimestamp.mockResolvedValue('2026-08-15T03:04:05Z');
		mocks.listMutations.mockResolvedValue([{}]);
		mocks.listConflicts.mockResolvedValue([{}, {}]);
		mocks.syncPendingCollectionChanges.mockResolvedValue(undefined);
	});

	afterEach(() => {
		vi.restoreAllMocks();
		vi.unstubAllGlobals();
	});

	it('refreshes user-scoped queue, conflict and snapshot metadata', async () => {
		const module = await import('$lib/offline/status.svelte');

		await module.refreshOfflineStatus();

		expect(module.getOfflineStatus()).toMatchObject({
			pendingCount: 1,
			conflictCount: 2,
			lastSyncedAt: '2026-08-15T03:04:05Z'
		});
		expect(mocks.listMutations).toHaveBeenCalledWith(7);
		expect(mocks.listConflicts).toHaveBeenCalledWith(7);
		expect(mocks.getSnapshotTimestamp).toHaveBeenCalledWith(7);
	});

	it('clears private status when no cached profile can be read', async () => {
		const module = await import('$lib/offline/status.svelte');
		await module.refreshOfflineStatus();
		mocks.getCachedProfile.mockRejectedValue(new Error('IndexedDB unavailable'));

		await module.refreshOfflineStatus();

		expect(module.getOfflineStatus()).toMatchObject({
			pendingCount: 0,
			conflictCount: 0,
			lastSyncedAt: null
		});
	});

	it('serializes manual synchronization and refreshes status afterwards', async () => {
		let finish: (() => void) | undefined;
		mocks.syncPendingCollectionChanges.mockImplementation(
			() => new Promise<void>((resolve) => (finish = resolve))
		);
		const module = await import('$lib/offline/status.svelte');

		const first = module.synchronizeNow();
		expect(module.getOfflineStatus().syncing).toBe(true);
		await module.synchronizeNow();
		expect(mocks.syncPendingCollectionChanges).toHaveBeenCalledOnce();
		finish?.();
		await first;

		expect(module.getOfflineStatus()).toMatchObject({
			syncing: false,
			syncError: null,
			pendingCount: 1
		});
	});

	it('reports typed and untyped synchronization failures', async () => {
		const module = await import('$lib/offline/status.svelte');
		mocks.syncPendingCollectionChanges.mockRejectedValueOnce(new Error('Server nicht erreichbar'));
		await module.synchronizeNow();
		expect(module.getOfflineStatus().syncError).toBe('Server nicht erreichbar');

		mocks.syncPendingCollectionChanges.mockRejectedValueOnce('unknown');
		await module.synchronizeNow();
		expect(module.getOfflineStatus().syncError).toBe('Synchronisierung fehlgeschlagen');
	});

	it('does not synchronize while connectivity is offline', async () => {
		vi.spyOn(navigator, 'onLine', 'get').mockReturnValue(false);
		const module = await import('$lib/offline/status.svelte');
		await module.refreshConnectivity();

		await module.synchronizeNow();

		expect(mocks.syncPendingCollectionChanges).not.toHaveBeenCalled();
	});

	it('wires browser, polling and cross-tab events once', async () => {
		let reportedOnline = true;
		vi.spyOn(navigator, 'onLine', 'get').mockImplementation(() => reportedOnline);
		Object.defineProperty(navigator, 'serviceWorker', {
			configurable: true,
			value: { controller: {} }
		});
		vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response(null, { status: 200 })));
		let intervalHandler: (() => void) | undefined;
		vi.spyOn(window, 'setInterval').mockImplementation((handler: TimerHandler) => {
			intervalHandler = handler as () => void;
			return globalThis.setTimeout(() => undefined, 0);
		});
		let channelMessage: (() => void) | undefined;
		const channelCreated = vi.fn();
		class BroadcastChannelMock {
			constructor(name: string) {
				channelCreated(name);
			}

			addEventListener(_type: string, handler: () => void) {
				channelMessage = handler;
			}
		}
		vi.stubGlobal('BroadcastChannel', BroadcastChannelMock);
		const module = await import('$lib/offline/status.svelte');

		module.initializeOfflineStatus();
		module.initializeOfflineStatus();
		await vi.waitFor(() => expect(mocks.getCachedProfile).toHaveBeenCalled());
		expect(window.setInterval).toHaveBeenCalledWith(expect.any(Function), 5_000);
		expect(channelCreated).toHaveBeenCalledWith('lilly-offline');

		window.dispatchEvent(new Event('offline'));
		expect(module.getOfflineStatus().online).toBe(false);
		reportedOnline = true;
		window.dispatchEvent(new Event('online'));
		await vi.waitFor(() => expect(mocks.syncPendingCollectionChanges).toHaveBeenCalledOnce());

		window.dispatchEvent(new Event('lilly:offline-change'));
		channelMessage?.();
		await vi.waitFor(() => expect(mocks.getCachedProfile.mock.calls.length).toBeGreaterThan(2));

		reportedOnline = false;
		window.dispatchEvent(new Event('offline'));
		intervalHandler?.();
		await vi.waitFor(() => expect(module.getOfflineStatus().online).toBe(false));
		Reflect.deleteProperty(navigator, 'serviceWorker');
	});
});
