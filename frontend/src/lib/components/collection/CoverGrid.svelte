<script lang="ts">
	import CoverCard from './CoverCard.svelte';
	import type { CollectionEntry } from '$lib/api/collection';

	interface Props {
		items: CollectionEntry[];
		loading?: boolean;
		empty_message?: string;
		onselect?: (entry: CollectionEntry) => void;
	}

	let {
		items,
		loading = false,
		empty_message = 'Keine Hefte gefunden.',
		onselect
	}: Props = $props();
</script>

{#if loading}
	<div
		class="grid gap-4"
		style="grid-template-columns: repeat(auto-fill, minmax(140px, 1fr));"
		data-testid="cover-grid-skeleton"
	>
		{#each Array(12) as _, i (i)}
			<div class="animate-pulse">
				<div class="aspect-[2/3] rounded-lg" style="background: var(--glass-border);"></div>
				<div class="h-3 mt-1 rounded" style="background: var(--glass-border); width: 80%;"></div>
			</div>
		{/each}
	</div>
{:else if items.length === 0}
	<div class="glass-elevated p-8 rounded-lg text-center" data-testid="cover-grid-empty">
		<p style="color: var(--text-secondary);">{empty_message}</p>
	</div>
{:else}
	<div
		class="grid gap-4"
		style="grid-template-columns: repeat(auto-fill, minmax(140px, 1fr));"
		data-testid="cover-grid"
	>
		{#each items as entry (entry.id || `missing-${entry.issue_id}`)}
			<CoverCard {entry} onclick={() => onselect?.(entry)} />
		{/each}
	</div>
{/if}
