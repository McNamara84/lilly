import { fetchMe, logout as apiLogout, refreshToken } from '$lib/api/auth';
import type { MeResponse } from '$lib/api/auth';

let user = $state<MeResponse | null>(null);
let isLoading = $state(true);

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
		}
	};
}

export async function initAuth(): Promise<void> {
	isLoading = true;
	try {
		user = await fetchMe();
	} catch {
		// Try refreshing the token once
		try {
			await refreshToken();
			user = await fetchMe();
		} catch {
			user = null;
		}
	} finally {
		isLoading = false;
	}
}

export async function performLogout(): Promise<void> {
	try {
		await apiLogout();
	} finally {
		user = null;
	}
}

export function setUser(newUser: MeResponse | null): void {
	user = newUser;
}
