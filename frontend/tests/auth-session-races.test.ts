import 'fake-indexeddb/auto';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { profile, snapshot } from './fixtures/offline';

vi.mock('#lib/api/auth.js', () => ({ fetchMe: vi.fn(), logout: vi.fn(), refreshToken: vi.fn() }));
vi.mock('#lib/api/collection.js', () => ({
	refreshOfflineSnapshot: vi.fn(),
	syncPendingCollectionChanges: vi.fn()
}));

describe('logout and in-flight authentication', () => {
	beforeEach(async () => {
		vi.restoreAllMocks();
		vi.resetAllMocks();
		vi.resetModules();
		localStorage.clear();
		sessionStorage.clear();
		const database = await import('#lib/offline/database.js');
		await database.resetOfflineDatabaseForTests();
		const api = await import('#lib/api/auth.js');
		vi.mocked(api.fetchMe).mockResolvedValue(profile(7));
		vi.mocked(api.logout).mockResolvedValue(undefined);
		const collection = await import('#lib/api/collection.js');
		vi.mocked(collection.refreshOfflineSnapshot).mockResolvedValue(undefined);
		vi.mocked(collection.syncPendingCollectionChanges).mockResolvedValue(null);
	});
	afterEach(() => {
		vi.restoreAllMocks();
		vi.unstubAllGlobals();
		window.history.replaceState({}, '', '/');
	});

	it('handles an account-deletion response when no local identity exists', async () => {
		const api = await import('#lib/api/auth.js');
		vi.mocked(api.fetchMe).mockRejectedValueOnce(
			Object.assign(new Error('deleted'), { code: 'ACCOUNT_DELETION_PENDING' })
		);
		vi.mocked(api.refreshToken).mockRejectedValueOnce(new Error('unauthorized'));
		const auth = await import('#lib/stores/auth.svelte.js');
		await auth.initAuth();
		expect(auth.getAuthState().isAuthenticated).toBe(false);
		expect(auth.getAuthState().isLoading).toBe(false);
	});

	it.each(['success', 'registered'])(
		'accepts a recent interactive OAuth %s after logout',
		async (outcome) => {
			const session = await import('#lib/offline/session.js');
			session.changeOfflineSession(7, true);
			sessionStorage.setItem('lilly-interactive-oauth', String(Date.now()));
			window.history.replaceState({}, '', `/?oauth=${outcome}`);
			const auth = await import('#lib/stores/auth.svelte.js');
			await auth.initAuth();
			expect(auth.getAuthState().user).toEqual(profile(7));
			expect(session.isOfflineUserBlocked(7)).toBe(false);
			expect(sessionStorage.getItem('lilly-interactive-oauth')).toBeNull();
			const confirmed = session.offlineSessionVersion();
			await auth.initAuth();
			expect(session.offlineSessionVersion()).toBe(confirmed);
		}
	);

	it('does not treat an expired OAuth marker as a new login', async () => {
		const session = await import('#lib/offline/session.js');
		session.changeOfflineSession(7, true);
		sessionStorage.setItem('lilly-interactive-oauth', String(Date.now() - 600_001));
		window.history.replaceState({}, '', '/?oauth=success');
		const auth = await import('#lib/stores/auth.svelte.js');
		await auth.initAuth();
		expect(auth.getAuthState().isAuthenticated).toBe(false);
		expect(session.isOfflineUserBlocked(7)).toBe(true);
	});

	it.each(['sessionStorage', 'location'])(
		'initializes without the browser global %s',
		async (name) => {
			vi.stubGlobal(name, undefined);
			const auth = await import('#lib/stores/auth.svelte.js');
			await auth.initAuth();
			expect(auth.getAuthState().user).toEqual(profile(7));
		}
	);

	it('stops initialization if another login supersedes interrupted cleanup', async () => {
		const database = await import('#lib/offline/database.js');
		const session = await import('#lib/offline/session.js');
		vi.spyOn(database, 'clearBlockedOfflineUsers').mockImplementationOnce(async () => {
			session.changeOfflineSession(8, false);
		});
		const auth = await import('#lib/stores/auth.svelte.js');
		await auth.initAuth();
		expect((await import('#lib/api/auth.js')).fetchMe).not.toHaveBeenCalled();
		expect(auth.getAuthState().isLoading).toBe(false);
	});

	it.each(['success', 'failure'])(
		'ignores a late authentication %s after logout',
		async (outcome) => {
			const auth = await import('#lib/stores/auth.svelte.js');
			const api = await import('#lib/api/auth.js');
			await auth.initAuth();
			const deferred = Promise.withResolvers<ReturnType<typeof profile>>();
			vi.mocked(api.fetchMe).mockReturnValueOnce(deferred.promise);
			const pending = auth.initAuth();
			await vi.waitFor(() => expect(api.fetchMe).toHaveBeenCalledTimes(2));
			await auth.performLogout();
			if (outcome === 'success') deferred.resolve(profile(7));
			else deferred.reject(new TypeError('late network error'));
			await pending;
			expect(auth.getAuthState().user).toBeNull();
			expect(api.refreshToken).not.toHaveBeenCalled();
		}
	);

	it('does not warm private data when logout interrupts profile persistence', async () => {
		const database = await import('#lib/offline/database.js');
		const saved = Promise.withResolvers<void>();
		const save = vi.spyOn(database, 'saveConfirmedProfile').mockReturnValueOnce(saved.promise);
		const auth = await import('#lib/stores/auth.svelte.js');
		const pending = auth.initAuth();
		await vi.waitFor(() => expect(save).toHaveBeenCalledOnce());
		await auth.performLogout();
		saved.resolve();
		await pending;
		expect(auth.getAuthState().user).toBeNull();
		expect(
			(await import('#lib/api/collection.js')).syncPendingCollectionChanges
		).not.toHaveBeenCalled();
	});

	it('still logs out if pending-change and conflict inspection fail', async () => {
		vi.stubGlobal('BroadcastChannel', undefined);
		const auth = await import('#lib/stores/auth.svelte.js');
		const database = await import('#lib/offline/database.js');
		await auth.initAuth();
		vi.spyOn(database, 'listMutations').mockRejectedValueOnce(new Error('storage failed'));
		vi.spyOn(database, 'listConflicts').mockRejectedValueOnce(new Error('storage failed'));
		expect(await auth.performLogout()).toBe(true);
		expect(auth.getAuthState().user).toBeNull();
		expect(await database.getCachedProfile()).toBeNull();
		expect(await auth.performLogout()).toBe(true);
	});

	it('cancels logout without losing pending changes when the user declines', async () => {
		const auth = await import('#lib/stores/auth.svelte.js');
		const database = await import('#lib/offline/database.js');
		const queue = await import('#lib/offline/queue.js');
		await auth.initAuth();
		await database.replaceOfflineSnapshot(snapshot(7));
		await queue.queueCollectionUpdate(7, snapshot(7).collection_entries[0].id, {
			notes: 'keep me'
		});
		vi.spyOn(window, 'confirm').mockReturnValue(false);
		expect(await auth.performLogout()).toBe(false);
		expect(auth.getAuthState().isAuthenticated).toBe(true);
		expect(await database.listMutations(7)).toHaveLength(1);
		expect((await import('#lib/api/auth.js')).logout).not.toHaveBeenCalled();
		vi.mocked(window.confirm).mockReturnValue(true);
		expect(await auth.performLogout()).toBe(true);
		expect(await database.getCachedProfile()).toBeNull();
	});

	it('ignores a successful auth response arriving after logout', async () => {
		const auth = await import('#lib/stores/auth.svelte.js');
		await auth.initAuth();
		let resolve!: (value: ReturnType<typeof profile>) => void;
		const api = await import('#lib/api/auth.js');
		vi.mocked(api.fetchMe).mockReturnValueOnce(
			new Promise((done) => {
				resolve = done;
			})
		);
		const pending = auth.initAuth();
		await auth.performLogout();
		resolve(profile(7));
		await pending;
		expect(auth.getAuthState().user).toBeNull();
		expect(await (await import('#lib/offline/database.js')).getCachedProfile()).toBeNull();
	});

	it('notifies other tabs before a slow remote logout completes', async () => {
		const postMessage = vi.fn();
		vi.stubGlobal(
			'BroadcastChannel',
			class {
				addEventListener() {}
				postMessage = postMessage;
				close() {}
			}
		);
		const auth = await import('#lib/stores/auth.svelte.js');
		await auth.initAuth();
		let finish!: () => void;
		const api = await import('#lib/api/auth.js');
		vi.mocked(api.logout).mockReturnValueOnce(
			new Promise((resolve) => {
				finish = resolve;
			})
		);
		const logout = auth.performLogout();
		await vi.waitFor(() =>
			expect(postMessage).toHaveBeenCalledWith({ type: 'logout', user_id: 7 })
		);
		expect(auth.getAuthState().isAuthenticated).toBe(false);
		finish();
		await logout;
	});

	it('blocks stale server cookies and offline fallback after failed cleanup until explicit login', async () => {
		const auth = await import('#lib/stores/auth.svelte.js');
		const database = await import('#lib/offline/database.js');
		await auth.initAuth();
		vi.spyOn(database, 'clearOfflineUserData').mockRejectedValueOnce(new Error('disk failed'));
		await expect(auth.performLogout()).rejects.toThrow('Websitedaten');
		expect(auth.getAuthState().logoutError).toContain('Websitedaten');
		await auth.initAuth();
		expect(auth.getAuthState().isAuthenticated).toBe(false);
		const api = await import('#lib/api/auth.js');
		vi.mocked(api.fetchMe).mockRejectedValueOnce(new TypeError('offline'));
		vi.mocked(api.refreshToken).mockRejectedValueOnce(new TypeError('offline'));
		await auth.initAuth();
		expect(auth.getAuthState().isAuthenticated).toBe(false);
		await auth.initAuth(true);
		expect(auth.getAuthState().user?.id).toBe(7);
	});

	it('deactivates only the matching account on another tab logout', async () => {
		let handler!: (event: MessageEvent) => void;
		vi.stubGlobal(
			'BroadcastChannel',
			class {
				addEventListener(_type: string, callback: typeof handler) {
					handler = callback;
				}
			}
		);
		const auth = await import('#lib/stores/auth.svelte.js');
		await auth.initAuth();
		handler({ data: { type: 'logout', user_id: 8 } } as MessageEvent);
		expect(auth.getAuthState().user?.id).toBe(7);
		const { offlineSessionVersion } = await import('#lib/offline/session.js');
		const version = offlineSessionVersion();
		handler({ data: { type: 'logout', user_id: 8 } } as MessageEvent);
		expect(offlineSessionVersion()).toBe(version);
		handler({ data: { type: 'logout', user_id: 7 } } as MessageEvent);
		expect(auth.getAuthState().isAuthenticated).toBe(false);
		expect(await (await import('#lib/offline/database.js')).getCachedProfile()).toBeNull();
	});

	it('retries interrupted cleanup before accepting the next account', async () => {
		const auth = await import('#lib/stores/auth.svelte.js');
		const database = await import('#lib/offline/database.js');
		await auth.initAuth();
		await database.replaceOfflineSnapshot(snapshot(7));
		vi.spyOn(database, 'clearOfflineUserData').mockRejectedValueOnce(new Error('tab navigated'));
		await expect(auth.performLogout()).rejects.toThrow('Websitedaten');
		const api = await import('#lib/api/auth.js');
		vi.mocked(api.fetchMe).mockResolvedValue(profile(8));
		await auth.initAuth(true);
		expect(auth.getAuthState().user?.id).toBe(8);
		// Inspect physical storage, not the read guard that hides a blocked account.
		const records = await new Promise<unknown[]>((resolve, reject) => {
			const open = indexedDB.open('lilly-offline');
			open.onerror = () => reject(open.error);
			open.onsuccess = () => {
				const db = open.result;
				const tx = db.transaction('collection_entries');
				const all = tx.objectStore('collection_entries').getAll();
				all.onsuccess = () => resolve(all.result);
				tx.oncomplete = () => db.close();
			};
		});
		expect(records).toEqual([]);
	});
});
