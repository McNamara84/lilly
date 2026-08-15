import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const syncPendingCollectionChanges = vi.hoisted(() => vi.fn());

vi.mock('$lib/api/collection', () => ({ syncPendingCollectionChanges }));

beforeEach(() => {
	syncPendingCollectionChanges.mockReset();
});

afterEach(() => {
	vi.restoreAllMocks();
	vi.unstubAllGlobals();
	vi.resetModules();
});

async function loadStatusWithNavigatorOnline(online: boolean) {
	vi.spyOn(window.navigator, 'onLine', 'get').mockReturnValue(online);
	return import('$lib/offline/status.svelte');
}

describe('offline connectivity status', () => {
	it('keeps offline connectivity visible together with queued changes or conflicts', async () => {
		const { formatOfflineStatusLabel } = await import('$lib/offline/status.svelte');
		expect(
			formatOfflineStatusLabel({
				online: false,
				syncing: false,
				pendingCount: 0,
				conflictCount: 0,
				syncError: null
			})
		).toBe('Offline');
		expect(
			formatOfflineStatusLabel({
				online: false,
				syncing: false,
				pendingCount: 2,
				conflictCount: 0,
				syncError: null
			})
		).toBe('Offline · 2 Änderung(en) ausstehend');
		expect(
			formatOfflineStatusLabel({
				online: false,
				syncing: false,
				pendingCount: 2,
				conflictCount: 1,
				syncError: null
			})
		).toBe('Offline · 1 Konflikt(e)');
	});

	it('prioritizes active online states and otherwise reports synchronization', async () => {
		const { formatOfflineStatusLabel, shouldProbeConnectivity } =
			await import('$lib/offline/status.svelte');
		const base = {
			online: true,
			syncing: false,
			pendingCount: 0,
			conflictCount: 0,
			syncError: null
		};
		expect(formatOfflineStatusLabel({ ...base, syncing: true })).toBe('Wird synchronisiert …');
		expect(formatOfflineStatusLabel({ ...base, conflictCount: 1 })).toBe('1 Konflikt(e)');
		expect(formatOfflineStatusLabel({ ...base, pendingCount: 3 })).toBe(
			'3 Änderung(en) ausstehend'
		);
		expect(formatOfflineStatusLabel({ ...base, syncError: 'failed' })).toBe(
			'Synchronisierung fehlgeschlagen'
		);
		expect(formatOfflineStatusLabel(base)).toBe('Synchronisiert');
		expect(shouldProbeConnectivity(true, false)).toBe(false);
		expect(shouldProbeConnectivity(false, false)).toBe(true);
		expect(shouldProbeConnectivity(true, true)).toBe(true);
	});

	it('does not probe the backend when the browser already reports offline', async () => {
		const fetchMock = vi.fn();
		vi.stubGlobal('fetch', fetchMock);
		const statusModule = await loadStatusWithNavigatorOnline(false);

		await expect(statusModule.refreshConnectivity()).resolves.toBe(false);
		expect(statusModule.getOfflineStatus().online).toBe(false);
		expect(fetchMock).not.toHaveBeenCalled();
	});

	it('marks an offline service-worker reload as unreachable when the health probe fails', async () => {
		const fetchMock = vi.fn().mockRejectedValue(new TypeError('Failed to fetch'));
		vi.stubGlobal('fetch', fetchMock);
		const statusModule = await loadStatusWithNavigatorOnline(true);

		await expect(statusModule.refreshConnectivity()).resolves.toBe(false);
		expect(statusModule.getOfflineStatus().online).toBe(false);
		expect(fetchMock).toHaveBeenCalledWith('/api/v1/health', {
			cache: 'no-store',
			credentials: 'same-origin'
		});
	});

	it('treats any HTTP response as network connectivity', async () => {
		vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response(null, { status: 503 })));
		const statusModule = await loadStatusWithNavigatorOnline(true);

		await expect(statusModule.refreshConnectivity()).resolves.toBe(true);
		expect(statusModule.getOfflineStatus().online).toBe(true);
	});

	it('synchronizes queued changes when a previously offline backend becomes reachable', async () => {
		const onlineSpy = vi.spyOn(window.navigator, 'onLine', 'get');
		onlineSpy.mockReturnValue(false);
		const statusModule = await import('$lib/offline/status.svelte');
		await statusModule.refreshConnectivity();

		onlineSpy.mockReturnValue(true);
		vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response(null, { status: 200 })));
		syncPendingCollectionChanges.mockResolvedValue(undefined);
		await statusModule.reconnectAndSynchronize();

		expect(syncPendingCollectionChanges).toHaveBeenCalledOnce();
		expect(statusModule.getOfflineStatus().online).toBe(true);
	});
});
