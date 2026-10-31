import { fetchMe, logout as apiLogout, refreshToken } from '#lib/api/auth.js';
import type { MeResponse } from '#lib/api/auth.js';
import { SvelteURLSearchParams } from 'svelte/reactivity';
import { refreshOfflineSnapshot, syncPendingCollectionChanges } from '#lib/api/collection.js';
import {
	clearOfflineUserData,
	clearBlockedOfflineUsers,
	getCachedProfile,
	saveConfirmedProfile
} from '#lib/offline/database.js';
import { isNetworkFailure } from '#lib/offline/network.js';
import { listMutations, listConflicts } from '#lib/offline/database.js';
import {
	changeOfflineSession,
	offlineSessionVersion,
	isCurrentOfflineSession,
	isOfflineUserBlocked
} from '#lib/offline/session.js';

let user = $state<MeResponse | null>(null);
let isLoading = $state(true);
let isOfflineSession = $state(false);
let logoutError = $state<string | null>(null);
let authBroadcastInitialized = false;

function initializeAuthBroadcast(): void {
	if (authBroadcastInitialized || typeof BroadcastChannel === 'undefined') return;
	authBroadcastInitialized = true;
	const channel = new BroadcastChannel('lilly-offline');
	channel.addEventListener('message', (event) => {
		if (!['account-deletion', 'logout'].includes(event.data?.type)) return;
		const deletedUserId = event.data.user_id as number | undefined;
		if (deletedUserId !== undefined) {
			// The sender already persisted the tombstone. A delayed notification must not
			// invalidate a newer login in another tab by creating a second session epoch.
			if (!isOfflineUserBlocked(deletedUserId)) changeOfflineSession(deletedUserId, true);
			void clearOfflineUserData(deletedUserId).catch(() => undefined);
		}
		if (deletedUserId === undefined || user?.id === deletedUserId) {
			user = null;
			isOfflineSession = false;
		}
	});
}

function warmOfflineData(): void {
	const version = offlineSessionVersion();
	void syncPendingCollectionChanges()
		.catch(() => undefined)
		.finally(() => {
			if (isCurrentOfflineSession(version)) return refreshOfflineSnapshot().catch(() => undefined);
		});
}

async function confirmOnlineUser(
	profile: MeResponse,
	version: string,
	interactive: boolean
): Promise<void> {
	if (!isCurrentOfflineSession(version)) return;
	if (isOfflineUserBlocked(profile.id) && !interactive) {
		user = null;
		return;
	}
	if (!version.startsWith(`active:${profile.id}:`)) changeOfflineSession(profile.id, false);
	const confirmedVersion = offlineSessionVersion();
	user = profile;
	isOfflineSession = false;
	await saveConfirmedProfile(profile).catch(() => undefined);
	if (isCurrentOfflineSession(confirmedVersion)) warmOfflineData();
}

async function restoreOfflineUser(): Promise<boolean> {
	const version = offlineSessionVersion();
	const cached = await getCachedProfile().catch(() => null);
	if (!cached || !isCurrentOfflineSession(version) || isOfflineUserBlocked(cached.id)) return false;
	user = cached;
	isOfflineSession = true;
	return true;
}

export function getAuthState() {
	return {
		get logoutError() {
			return logoutError;
		},
		get user() {
			return user;
		},
		get isLoading() {
			return isLoading;
		},
		get isAuthenticated() {
			return user !== null;
		},
		get isAdmin() {
			return user?.role === 'admin';
		},
		get isOfflineSession() {
			return isOfflineSession;
		}
	};
}

export async function initAuth(interactive = false): Promise<void> {
	initializeAuthBroadcast();
	if (typeof sessionStorage !== 'undefined' && typeof location !== 'undefined') {
		const started = Number(sessionStorage.getItem('lilly-interactive-oauth') ?? 0);
		if (
			['success', 'registered'].includes(
				new SvelteURLSearchParams(location.search).get('oauth') ?? ''
			) &&
			started > Date.now() - 600_000
		) {
			interactive = true;
			sessionStorage.removeItem('lilly-interactive-oauth');
		}
	}
	isLoading = true;
	const version = offlineSessionVersion();
	try {
		await clearBlockedOfflineUsers().catch(() => undefined);
		if (!isCurrentOfflineSession(version)) return;
		await confirmOnlineUser(await fetchMe(), version, interactive);
	} catch (initialError) {
		if (!isCurrentOfflineSession(version)) return;
		if (hasAccountDeletionCode(initialError)) await clearCurrentOfflineData();
		if (isNetworkFailure(initialError) && (await restoreOfflineUser())) {
			isLoading = false;
			return;
		}
		// Try refreshing the token once
		try {
			await refreshToken();
			await confirmOnlineUser(await fetchMe(), version, interactive);
		} catch (refreshError) {
			if (!isCurrentOfflineSession(version)) return;
			if (hasAccountDeletionCode(refreshError)) await clearCurrentOfflineData();
			if (!isNetworkFailure(refreshError) || !(await restoreOfflineUser())) {
				user = null;
				isOfflineSession = false;
			}
		}
	} finally {
		isLoading = false;
	}
}

function hasAccountDeletionCode(cause: unknown): boolean {
	const code = (cause as { code?: string } | null)?.code;
	return code === 'ACCOUNT_DELETION_PENDING' || code === 'ACCOUNT_DELETION_WINDOW_EXPIRED';
}

async function clearCurrentOfflineData(): Promise<void> {
	const userId = user?.id ?? (await getCachedProfile().catch(() => null))?.id;
	if (userId !== undefined) {
		changeOfflineSession(userId, true);
		user = null;
		isOfflineSession = false;
		await clearOfflineUserData(userId).catch(() => undefined);
	}
}

export async function deactivateAccountLocally(): Promise<void> {
	const userId = user?.id ?? (await getCachedProfile().catch(() => null))?.id;
	if (userId !== undefined) changeOfflineSession(userId, true);
	user = null;
	isOfflineSession = false;
	let offlineCleanupFailed = false;
	if (userId !== undefined) {
		try {
			await clearOfflineUserData(userId);
		} catch {
			offlineCleanupFailed = true;
		}
	}
	if (typeof navigator !== 'undefined' && 'serviceWorker' in navigator) {
		void navigator.serviceWorker.ready
			.then((registration) => {
				registration.active?.postMessage({ type: 'PURGE_PRIVATE_DATA' });
			})
			.catch(() => undefined);
	}
	if (typeof BroadcastChannel !== 'undefined') {
		const channel = new BroadcastChannel('lilly-offline');
		channel.postMessage({ type: 'account-deletion', user_id: userId });
		channel.close();
	}
	if (offlineCleanupFailed) {
		throw new Error(
			'Lokale Kontodaten konnten nicht vollständig gelöscht werden. Bitte versuche es erneut.'
		);
	}
	user = null;
	isOfflineSession = false;
}

export async function performLogout(): Promise<boolean> {
	logoutError = null;
	const userId = user?.id;
	if (userId !== undefined) {
		const [pending, conflicts] = await Promise.all([
			listMutations(userId).catch(() => []),
			listConflicts(userId).catch(() => [])
		]);
		if (
			(pending.length || conflicts.length) &&
			!window.confirm(
				'Es gibt noch nicht übertragene Änderungen oder ungelöste Konflikte. Beim Abmelden werden diese lokalen Daten gelöscht. Trotzdem abmelden?'
			)
		)
			return false;
		changeOfflineSession(userId, true);
	}
	user = null;
	isOfflineSession = false;
	if (typeof BroadcastChannel !== 'undefined') {
		const channel = new BroadcastChannel('lilly-offline');
		channel.postMessage({ type: 'logout', user_id: userId });
		channel.close();
	}
	let remoteError: unknown;
	try {
		await apiLogout();
	} catch (error) {
		remoteError = error;
	}
	if (userId !== undefined) {
		try {
			await clearOfflineUserData(userId);
		} catch {
			logoutError =
				'Du bist abgemeldet. Lokale Kontodaten konnten nicht vollständig gelöscht werden. Bitte lösche die Websitedaten dieses Browsers.';
			throw new Error(logoutError);
		}
	}
	if (remoteError) {
		logoutError =
			'Du bist auf diesem Gerät abgemeldet. Die Abmeldung beim Server konnte nicht bestätigt werden.';
		throw remoteError;
	}
	return true;
}

export function setUser(newUser: MeResponse | null): void {
	if (newUser) changeOfflineSession(newUser.id, false);
	user = newUser;
	isOfflineSession = false;
	if (newUser) void saveConfirmedProfile(newUser).catch(() => undefined);
}
