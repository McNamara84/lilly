import 'fake-indexeddb/auto';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { profile, snapshot } from './fixtures/offline';

vi.mock('#lib/api/auth.js', () => ({ fetchMe: vi.fn(), logout: vi.fn(), refreshToken: vi.fn() }));
vi.mock('#lib/api/collection.js', () => ({
	refreshOfflineSnapshot: vi.fn(),
	syncPendingCollectionChanges: vi.fn()
}));

describe('logout and in-flight authentication', () => {
	it.each(['confirmation', 'direct', 'oauth', 'refresh'] as const)(
		'clears a previous logout error after a successful %s login',
		async (path) => {
			const auth = await import('#lib/stores/auth.svelte.js');
			const api = await import('#lib/api/auth.js');
			await auth.initAuth();
			vi.mocked(api.logout).mockRejectedValueOnce(new Error('server unavailable'));
			await expect(auth.performLogout()).rejects.toThrow('server unavailable');
			expect(auth.getAuthState().logoutError).toContain('nicht bestätigt');
			if (path === 'direct') auth.setUser(profile(7));
			else if (path === 'oauth') {
				sessionStorage.setItem('lilly-interactive-oauth', String(Date.now()));
				window.history.replaceState({}, '', '/?oauth=success');
				await auth.initAuth();
			} else {
				if (path === 'refresh') vi.mocked(api.fetchMe).mockRejectedValueOnce(new Error('expired'));
				await auth.initAuth(true);
			}
			expect(auth.getAuthState().user).toEqual(profile(7));
			expect(auth.getAuthState().logoutError).toBeNull();
		}
	);

	it('preserves the logout error until a new login actually succeeds', async () => {
		const auth = await import('#lib/stores/auth.svelte.js');
		const api = await import('#lib/api/auth.js');
		await auth.initAuth();
		vi.mocked(api.logout).mockRejectedValueOnce(new Error('server unavailable'));
		await expect(auth.performLogout()).rejects.toThrow();
		const error = auth.getAuthState().logoutError;
		auth.setUser(null);
		vi.mocked(api.fetchMe).mockRejectedValueOnce(new Error('login failed'));
		vi.mocked(api.refreshToken).mockRejectedValueOnce(new Error('refresh failed'));
		await auth.initAuth(true);
		expect(auth.getAuthState().isAuthenticated).toBe(false);
		expect(auth.getAuthState().logoutError).toBe(error);
	});

	it('does not restore an old logout error when the request fails after a new login', async () => {
		const auth = await import('#lib/stores/auth.svelte.js');
		const api = await import('#lib/api/auth.js');
		await auth.initAuth();
		const remote = Promise.withResolvers<void>();
		vi.mocked(api.logout).mockReturnValueOnce(remote.promise);
		const pending = auth.performLogout();
		await vi.waitFor(() => expect(api.logout).toHaveBeenCalledOnce());
		await auth.initAuth(true);
		remote.reject(new Error('old logout failed'));
		await expect(pending).rejects.toThrow('old logout failed');
		expect(auth.getAuthState().user).toEqual(profile(7));
		expect(auth.getAuthState().logoutError).toBeNull();
	});

	it('does not show a late cleanup error in a newer authenticated session', async () => {
		// Delay this tab's logout cleanup, independently of the broadcast receiver.
		vi.stubGlobal('BroadcastChannel', undefined);
		const auth = await import('#lib/stores/auth.svelte.js');
		const database = await import('#lib/offline/database.js');
		await auth.initAuth();
		const cleanup = Promise.withResolvers<void>();
		const clear = vi.spyOn(database, 'clearOfflineUserData').mockReturnValueOnce(cleanup.promise);
		const pending = auth.performLogout();
		await vi.waitFor(() => expect(clear).toHaveBeenCalledOnce());
		await auth.initAuth(true);
		cleanup.reject(new Error('old cleanup failed'));
		await expect(pending).rejects.toThrow('Websitedaten');
		expect(auth.getAuthState().user).toEqual(profile(7));
		expect(auth.getAuthState().logoutError).toBeNull();
	});
	it('does not clear a fresh login when an older remote logout finishes late', async () => {
		const auth = await import('#lib/stores/auth.svelte.js');
		const api = await import('#lib/api/auth.js');
		const database = await import('#lib/offline/database.js');
		await auth.initAuth();
		const remote = Promise.withResolvers<void>();
		vi.mocked(api.logout).mockReturnValueOnce(remote.promise);
		const pending = auth.performLogout();
		await vi.waitFor(() => expect(api.logout).toHaveBeenCalledOnce());
		await auth.initAuth(true);
		await database.replaceOfflineSnapshot(snapshot(7));
		remote.resolve();
		await pending;
		expect(auth.getAuthState().user).toEqual(profile(7));
		expect(await database.getCachedProfile()).toEqual(profile(7));
		expect(await database.listStoredCollectionEntries(7)).toEqual(snapshot(7).collection_entries);
	});

	it('cancels a pending logout when another login supersedes its queue inspection', async () => {
		const auth = await import('#lib/stores/auth.svelte.js');
		const database = await import('#lib/offline/database.js');
		const api = await import('#lib/api/auth.js');
		await auth.initAuth();
		const inspect = Promise.withResolvers<Awaited<ReturnType<typeof database.listMutations>>>();
		vi.spyOn(database, 'listMutations').mockReturnValueOnce(inspect.promise);
		const pending = auth.performLogout();
		auth.setUser(profile(8));
		inspect.resolve([]);
		expect(await pending).toBe(false);
		expect(api.logout).not.toHaveBeenCalled();
		expect(auth.getAuthState().user).toEqual(profile(8));
	});
	it.each(['logout', 'account-deletion'])(
		'ignores a delayed %s from a previous login of the same account',
		async (type) => {
			let message!: (event: MessageEvent) => void;
			vi.stubGlobal(
				'BroadcastChannel',
				class {
					addEventListener(_type: string, callback: typeof message) {
						message = callback;
					}
				}
			);
			const auth = await import('#lib/stores/auth.svelte.js');
			const database = await import('#lib/offline/database.js');
			const session = await import('#lib/offline/session.js');
			await auth.initAuth();
			const previousVersion = session.offlineSessionVersion();
			session.changeOfflineSession(7, true);
			const version = session.offlineSessionVersion();
			await database.clearOfflineUserData(7);
			await auth.initAuth(true);
			await database.replaceOfflineSnapshot(snapshot(7));
			const freshVersion = session.offlineSessionVersion();
			const clear = vi.spyOn(database, 'clearOfflineUserData');
			message({
				data: { type, user_id: 7, previous_version: previousVersion, session_version: version }
			} as MessageEvent);
			expect(auth.getAuthState().user).toEqual(profile(7));
			expect(session.offlineSessionVersion()).toBe(freshVersion);
			expect(clear).not.toHaveBeenCalled();
			expect(await database.getCachedProfile()).toEqual(profile(7));
			expect(await database.listStoredCollectionEntries(7)).toEqual(snapshot(7).collection_entries);
		}
	);
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

	it('does not deactivate a new login while identifying the cached account to clean up', async () => {
		const auth = await import('#lib/stores/auth.svelte.js');
		const database = await import('#lib/offline/database.js');
		const api = await import('#lib/api/auth.js');
		vi.mocked(api.fetchMe).mockRejectedValueOnce(
			Object.assign(new Error('deleted'), { code: 'ACCOUNT_DELETION_WINDOW_EXPIRED' })
		);
		vi.spyOn(database, 'getCachedProfile').mockImplementationOnce(async () => {
			auth.setUser(profile(8));
			return profile(7);
		});
		const clear = vi.spyOn(database, 'clearOfflineUserData');
		await auth.initAuth();
		expect(clear).not.toHaveBeenCalled();
		expect(auth.getAuthState().user).toEqual(profile(8));
		expect(await database.getCachedProfile()).toEqual(profile(8));
		expect(api.refreshToken).not.toHaveBeenCalled();
	});

	it('keeps a deleted account blocked when its local cleanup fails', async () => {
		const auth = await import('#lib/stores/auth.svelte.js');
		const database = await import('#lib/offline/database.js');
		const api = await import('#lib/api/auth.js');
		await auth.initAuth();
		vi.mocked(api.fetchMe).mockRejectedValueOnce(
			Object.assign(new Error('deleted'), { code: 'ACCOUNT_DELETION_PENDING' })
		);
		vi.spyOn(database, 'clearOfflineUserData').mockRejectedValueOnce(new Error('disk unavailable'));
		await auth.initAuth();
		expect(auth.getAuthState().user).toBeNull();
		expect((await import('#lib/offline/session.js')).isOfflineUserBlocked(7)).toBe(true);
		expect(api.refreshToken).not.toHaveBeenCalled();
	});

	it('ignores a late refresh failure after another login has succeeded', async () => {
		const auth = await import('#lib/stores/auth.svelte.js');
		const api = await import('#lib/api/auth.js');
		await auth.initAuth();
		vi.mocked(api.fetchMe).mockRejectedValueOnce(new Error('unauthorized'));
		vi.mocked(api.refreshToken).mockImplementationOnce(async () => {
			auth.setUser(profile(8));
			throw new Error('late refresh failure');
		});
		await auth.initAuth();
		expect(auth.getAuthState().user).toEqual(profile(8));
		expect(await (await import('#lib/offline/database.js')).getCachedProfile()).toEqual(profile(8));
	});

	it.each(['initial', 'refresh'] as const)(
		'keeps a fresh login and its data when an %s account-deletion cleanup resumes late',
		async (phase) => {
			const auth = await import('#lib/stores/auth.svelte.js');
			const api = await import('#lib/api/auth.js');
			const database = await import('#lib/offline/database.js');
			await auth.initAuth();
			await database.replaceOfflineSnapshot(snapshot(7));
			const deletion = Object.assign(new Error('deleted'), { code: 'ACCOUNT_DELETION_PENDING' });
			if (phase === 'initial') vi.mocked(api.fetchMe).mockRejectedValueOnce(deletion);
			else {
				vi.mocked(api.fetchMe).mockRejectedValueOnce(new Error('unauthorized'));
				vi.mocked(api.refreshToken).mockRejectedValueOnce(deletion);
			}
			const get = IDBObjectStore.prototype.get;
			let relogged = false;
			vi.spyOn(IDBObjectStore.prototype, 'get').mockImplementation(function (
				this: IDBObjectStore,
				key
			) {
				const request = get.call(this, key);
				if (!relogged && this.name === 'metadata' && key === 'active_user_id') {
					relogged = true;
					request.addEventListener('success', () => auth.setUser(profile(7)), { once: true });
				}
				return request;
			});
			await auth.initAuth();
			expect(relogged).toBe(true);
			expect(auth.getAuthState().user).toEqual(profile(7));
			expect(await database.getCachedProfile()).toEqual(profile(7));
			expect(await database.listStoredCollectionEntries(7)).toEqual(snapshot(7).collection_entries);
			expect(api.refreshToken).toHaveBeenCalledTimes(phase === 'initial' ? 0 : 1);
		}
	);

	it('does not purge a newer login when service-worker readiness delays account cleanup', async () => {
		const ready = Promise.withResolvers<ServiceWorkerRegistration>();
		const postMessage = vi.fn();
		vi.stubGlobal('navigator', { serviceWorker: { ready: ready.promise } });
		const auth = await import('#lib/stores/auth.svelte.js');
		await auth.initAuth();
		await auth.deactivateAccountLocally();
		await auth.initAuth(true);
		ready.resolve({ active: { postMessage } } as unknown as ServiceWorkerRegistration);
		await ready.promise;
		expect(postMessage).not.toHaveBeenCalled();
		expect(auth.getAuthState().user).toEqual(profile(7));
		expect(await (await import('#lib/offline/database.js')).getCachedProfile()).toEqual(profile(7));
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
			expect(postMessage).toHaveBeenCalledWith(
				expect.objectContaining({
					type: 'logout',
					user_id: 7,
					previous_version: expect.stringMatching(/^active:7:/),
					session_version: expect.stringMatching(/^blocked:7:/)
				})
			)
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
		const { sessionEndMessage } = await import('./fixtures/session-end');
		const notification = sessionEndMessage('logout', 8);
		handler(notification);
		expect(auth.getAuthState().user?.id).toBe(7);
		const { offlineSessionVersion } = await import('#lib/offline/session.js');
		const version = offlineSessionVersion();
		handler(notification);
		expect(offlineSessionVersion()).toBe(version);
		handler(sessionEndMessage('logout', 7));
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
