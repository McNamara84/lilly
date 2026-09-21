<script lang="ts">
	import type { Snippet } from 'svelte';
	import { resolve } from '$app/paths';
	import { getOfflineStatus } from '$lib/offline/status.svelte';

	let { feature, children }: { feature: string; children: Snippet } = $props();
	const status = getOfflineStatus();
</script>

{#if status.online}
	{@render children()}
{:else}
	<div class="mx-auto max-w-xl px-4 py-12 text-center" data-testid="offline-unavailable-page">
		<h1 class="mb-3 text-2xl font-bold" style="color: var(--text-primary);">
			Offline nicht verfügbar
		</h1>
		<p class="mb-6 text-sm" style="color: var(--text-secondary);" role="status">
			{feature} benötigt eine Internetverbindung. Sobald du wieder online bist, geht es hier automatisch
			weiter.
		</p>
		<a
			class="underline text-sm"
			href={resolve('/collection')}
			data-testid="offline-collection-link"
		>
			Zur Sammlung (offline verfügbar)
		</a>
	</div>
{/if}
