import 'fake-indexeddb/auto';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { profile } from './fixtures/offline';

vi.mock('$lib/api/auth', () => ({
	fetchMe: vi.fn(),
	logout: vi.fn(),
	refreshToken: vi.fn()
}));

vi.mock('$lib/api/collection', () => ({
	refreshOfflineSnapshot: vi.fn().mockResolvedValue(undefined),
	syncPendingCollectionChanges: vi.fn().mockResolvedValue(null)
}));

describe('offline authentication context', () => {
	beforeEach(async () => {
		vi.clearAllMocks();
		vi.unstubAllGlobals();
		vi.resetModules();
		const { resetOfflineDatabaseForTests } = await import('$lib/offline/database');
		await resetOfflineDatabaseForTests();
	});

	it('purges the matching account in other tabs after an account-deletion broadcast', async () => {
		let messageHandler: ((event: MessageEvent) => void) | undefined;
		class BroadcastChannelMock {
			addEventListener(_type: string, handler: (event: MessageEvent) => void) {
				messageHandler = handler;
			}
		}
		vi.stubGlobal('BroadcastChannel', BroadcastChannelMock);
		const { fetchMe } = await import('$lib/api/auth');
		vi.mocked(fetchMe).mockResolvedValue(profile(7));
		const { getAuthState, initAuth } = await import('$lib/stores/auth.svelte');
		await initAuth();

		messageHandler?.({ data: { type: 'logout', user_id: 7 } } as MessageEvent);
		expect(getAuthState().user).toEqual(profile(7));
		messageHandler?.({ data: { type: 'account-deletion', user_id: 8 } } as MessageEvent);
		expect(getAuthState().user).toEqual(profile(7));
		messageHandler?.({ data: { type: 'account-deletion', user_id: 7 } } as MessageEvent);

		await vi.waitFor(() => expect(getAuthState().user).toBeNull());
		const { getCachedProfile } = await import('$lib/offline/database');
		await vi.waitFor(async () => expect(await getCachedProfile()).toBeNull());
	});

	it('purges IndexedDB, service-worker caches, and other tabs after local deactivation', async () => {
		const { saveConfirmedProfile } = await import('$lib/offline/database');
		await saveConfirmedProfile(profile(7));
		const serviceWorkerPost = vi.fn();
		const broadcastPost = vi.fn();
		const broadcastClose = vi.fn();
		class BroadcastChannelMock {
			addEventListener() {}
			postMessage = broadcastPost;
			close = broadcastClose;
		}
		vi.stubGlobal('navigator', {
			serviceWorker: {
				ready: Promise.resolve({ active: { postMessage: serviceWorkerPost } })
			}
		});
		vi.stubGlobal('BroadcastChannel', BroadcastChannelMock);
		const { deactivateAccountLocally, getAuthState } = await import('$lib/stores/auth.svelte');

		await deactivateAccountLocally();

		const { getCachedProfile } = await import('$lib/offline/database');
		expect(await getCachedProfile()).toBeNull();
		expect(getAuthState().isAuthenticated).toBe(false);
		expect(getAuthState().isOfflineSession).toBe(false);
		expect(serviceWorkerPost).toHaveBeenCalledWith({ type: 'PURGE_PRIVATE_DATA' });
		expect(broadcastPost).toHaveBeenCalledWith({ type: 'account-deletion', user_id: 7 });
		expect(broadcastClose).toHaveBeenCalledOnce();
	});

	it('removes a cached identity when the backend reports pending account deletion', async () => {
		const { saveConfirmedProfile } = await import('$lib/offline/database');
		await saveConfirmedProfile(profile(7));
		const { fetchMe, refreshToken } = await import('$lib/api/auth');
		vi.mocked(fetchMe).mockRejectedValue(
			Object.assign(new Error('Account deletion is pending'), {
				code: 'ACCOUNT_DELETION_PENDING'
			})
		);
		vi.mocked(refreshToken).mockRejectedValue(new Error('Refresh rejected'));
		const { getAuthState, initAuth } = await import('$lib/stores/auth.svelte');

		await initAuth();

		const { getCachedProfile } = await import('$lib/offline/database');
		expect(await getCachedProfile()).toBeNull();
		expect(getAuthState().user).toBeNull();
	});

	it('restores only a confirmed cached user after a network failure', async () => {
		const { saveConfirmedProfile } = await import('$lib/offline/database');
		await saveConfirmedProfile(profile(7));
		const { fetchMe, refreshToken } = await import('$lib/api/auth');
		vi.mocked(fetchMe).mockRejectedValue(new TypeError('Network unavailable'));

		const { getAuthState, initAuth } = await import('$lib/stores/auth.svelte');
		await initAuth();

		expect(getAuthState().user).toEqual(profile(7));
		expect(getAuthState().isOfflineSession).toBe(true);
		expect(refreshToken).not.toHaveBeenCalled();
	});

	it('does not use cached identity for an authentication rejection', async () => {
		const { saveConfirmedProfile } = await import('$lib/offline/database');
		await saveConfirmedProfile(profile(7));
		const { fetchMe, refreshToken } = await import('$lib/api/auth');
		vi.mocked(fetchMe).mockRejectedValue(new Error('Unauthorized'));
		vi.mocked(refreshToken).mockRejectedValue(new Error('Refresh rejected'));

		const { getAuthState, initAuth } = await import('$lib/stores/auth.svelte');
		await initAuth();

		expect(getAuthState().user).toBeNull();
		expect(getAuthState().isOfflineSession).toBe(false);
	});

	it('clears private offline data even when remote logout fails', async () => {
		const { fetchMe, logout } = await import('$lib/api/auth');
		vi.mocked(fetchMe).mockResolvedValue(profile(7));
		vi.mocked(logout).mockRejectedValue(new TypeError('Network unavailable'));
		const { initAuth, performLogout } = await import('$lib/stores/auth.svelte');
		await initAuth();

		await expect(performLogout()).rejects.toThrow('Network unavailable');
		const { getCachedProfile } = await import('$lib/offline/database');
		expect(await getCachedProfile()).toBeNull();
	});

	it('refreshes the snapshot even when sending pending changes fails', async () => {
		const { fetchMe } = await import('$lib/api/auth');
		const { refreshOfflineSnapshot, syncPendingCollectionChanges } =
			await import('$lib/api/collection');
		vi.mocked(fetchMe).mockResolvedValue(profile(7));
		vi.mocked(syncPendingCollectionChanges).mockRejectedValue(new TypeError('Network unavailable'));
		vi.mocked(refreshOfflineSnapshot).mockResolvedValue(undefined);
		const { initAuth } = await import('$lib/stores/auth.svelte');

		await initAuth();

		await vi.waitFor(() => expect(refreshOfflineSnapshot).toHaveBeenCalledOnce());
	});
});
