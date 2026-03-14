import { redirect } from '@sveltejs/kit';
import { getAuthState } from '$lib/stores/auth.svelte';

export function load() {
	const auth = getAuthState();

	if (!auth.isAuthenticated && !auth.isLoading) {
		redirect(302, '/login');
	}
}
