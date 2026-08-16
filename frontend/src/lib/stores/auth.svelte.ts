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
	isLoading = true;
	try {
		await confirmOnlineUser(await fetchMe());
	} catch (initialError) {
		if (isNetworkFailure(initialError) && (await restoreOfflineUser())) {
			isLoading = false;
			return;
		}
		// Try refreshing the token once
		try {
			await refreshToken();
			await confirmOnlineUser(await fetchMe());
		} catch (refreshError) {
			if (!isNetworkFailure(refreshError) || !(await restoreOfflineUser())) {
				user = null;
				isOfflineSession = false;
			}
		}
	} finally {
		isLoading = false;
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
