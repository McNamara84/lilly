<script lang="ts">
	import { getAuthState } from '$lib/stores/auth.svelte';
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';

	let { children } = $props();

	const auth = getAuthState();

	$effect(() => {
		if (!auth.isLoading) {
			if (!auth.isAuthenticated) {
				goto(resolve('/login'));
			} else if (!auth.isAdmin) {
				goto(resolve('/'));
			}
		}
	});
</script>

{#if auth.isAdmin}
	<div class="min-h-[calc(100vh-3.5rem)] px-4 py-8 sm:px-6 lg:px-8">
		<nav class="mb-6 flex gap-4" aria-label="Admin-Navigation">
			<a
				href={resolve('/admin/series')}
				class="text-sm font-medium px-3 py-1.5 rounded-lg transition-colors"
				style="color: var(--text-secondary); background-color: var(--surface-raised);"
				data-testid="admin-nav-series"
			>
				Serien
			</a>
			<a
				href={resolve('/admin/import')}
				class="text-sm font-medium px-3 py-1.5 rounded-lg transition-colors"
				style="color: var(--text-secondary); background-color: var(--surface-raised);"
				data-testid="admin-nav-import"
			>
				Import
			</a>
		</nav>
		{@render children()}
	</div>
{/if}
