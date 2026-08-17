import { fetchMe, logout as apiLogout, refreshToken } from '$lib/api/auth';
import type { MeResponse } from '$lib/api/auth';
import { refreshOfflineSnapshot, syncPendingCollectionChanges } from '$lib/api/collection';
import {
	clearOfflineUserData,
	getCachedProfile,
	saveConfirmedProfile
} from '$lib/offline/database';
import { isNetworkFailure } from '$lib/offline/network';

let user = $state<MeResponse | null>(null);
let isLoading = $state(true);
let isOfflineSession = $state(false);
let authBroadcastInitialized = false;

function initializeAuthBroadcast(): void {
	if (authBroadcastInitialized || typeof BroadcastChannel === 'undefined') return;
	authBroadcastInitialized = true;
	const channel = new BroadcastChannel('lilly-offline');
	channel.addEventListener('message', (event) => {
		if (event.data?.type !== 'account-deletion') return;
		const deletedUserId = event.data.user_id as number | undefined;
		if (deletedUserId !== undefined) {
			void clearOfflineUserData(deletedUserId).catch(() => undefined);
		}
		if (deletedUserId === undefined || user?.id === deletedUserId) {
			user = null;
			isOfflineSession = false;
		}
	});
}

function warmOfflineData(): void {
	void syncPendingCollectionChanges()
		.catch(() => undefined)
		.finally(() => refreshOfflineSnapshot().catch(() => undefined));
}

async function confirmOnlineUser(profile: MeResponse): Promise<void> {
	user = profile;
	isOfflineSession = false;
	await saveConfirmedProfile(profile).catch(() => undefined);
	warmOfflineData();
}

async function restoreOfflineUser(): Promise<boolean> {
	const cached = await getCachedProfile().catch(() => null);
	if (!cached) return false;
	user = cached;
	isOfflineSession = true;
	return true;
}

export function getAuthState() {
	return {
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

export async function initAuth(): Promise<void> {
	initializeAuthBroadcast();
	isLoading = true;
	try {
		await confirmOnlineUser(await fetchMe());
	} catch (initialError) {
		if (hasAccountDeletionCode(initialError)) await clearCurrentOfflineData();
		if (isNetworkFailure(initialError) && (await restoreOfflineUser())) {
			isLoading = false;
			return;
		}
		// Try refreshing the token once
		try {
			await refreshToken();
			await confirmOnlineUser(await fetchMe());
		} catch (refreshError) {
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
	if (userId !== undefined) await clearOfflineUserData(userId).catch(() => undefined);
}

export async function deactivateAccountLocally(): Promise<void> {
	const userId = user?.id ?? (await getCachedProfile().catch(() => null))?.id;
	user = null;
	isOfflineSession = false;
	if (userId !== undefined) await clearOfflineUserData(userId).catch(() => undefined);
	if (typeof navigator !== 'undefined' && 'serviceWorker' in navigator) {
		const registration = await navigator.serviceWorker.ready.catch(() => null);
		registration?.active?.postMessage({ type: 'PURGE_PRIVATE_DATA' });
	}
	if (typeof BroadcastChannel !== 'undefined') {
		const channel = new BroadcastChannel('lilly-offline');
		channel.postMessage({ type: 'account-deletion', user_id: userId });
		channel.close();
	}
}

export async function performLogout(): Promise<void> {
	const userId = user?.id;
	try {
		await apiLogout();
	} finally {
		user = null;
		isOfflineSession = false;
		if (userId !== undefined) await clearOfflineUserData(userId).catch(() => undefined);
		if (typeof BroadcastChannel !== 'undefined') {
			const channel = new BroadcastChannel('lilly-offline');
			channel.postMessage({ type: 'logout', user_id: userId });
			channel.close();
		}
	}
}

export function setUser(newUser: MeResponse | null): void {
	user = newUser;
	isOfflineSession = false;
	if (newUser) void saveConfirmedProfile(newUser).catch(() => undefined);
}
