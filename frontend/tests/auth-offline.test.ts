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
		vi.resetModules();
		const { resetOfflineDatabaseForTests } = await import('$lib/offline/database');
		await resetOfflineDatabaseForTests();
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
});
