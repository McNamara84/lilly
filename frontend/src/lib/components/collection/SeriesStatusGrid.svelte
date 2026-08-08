<script lang="ts">
	import type { CollectionEntry, CollectionStatus } from '$lib/api/collection';
	import type { Issue } from '$lib/api/series';
	import {
		buildSeriesGridItems,
		COLLECTION_STATUS_PRESENTATION
	} from '$lib/collection/series-grid';
	import CollectionStatusLegend from './CollectionStatusLegend.svelte';

	interface Props {
		issues: Issue[];
		entries: CollectionEntry[];
		onselect: (issue: Issue, entry: CollectionEntry | null, trigger: HTMLButtonElement) => void;
	}

	let { issues, entries, onselect }: Props = $props();
	const items = $derived(buildSeriesGridItems(issues, entries));

	function cellStyle(status: CollectionStatus): string {
		if (status === 'missing') {
			return 'background: var(--glass); color: var(--text-secondary); border: 2px dashed var(--glass-border);';
		}
		if (status === 'wanted') {
			return 'background: repeating-linear-gradient(135deg, var(--color-status-wanted), var(--color-status-wanted) 6px, color-mix(in srgb, var(--color-status-wanted) 65%, transparent) 6px, color-mix(in srgb, var(--color-status-wanted) 65%, transparent) 12px); color: #000; border: 2px solid var(--color-status-wanted);';
		}
		return `background: var(--color-status-${status}); color: #000; border: 2px solid var(--color-status-${status});`;
	}
</script>

<div data-testid="series-status-grid-container">
	<div
		class="grid gap-2 sm:gap-3"
		style="grid-template-columns: repeat(auto-fill, minmax(52px, 1fr));"
		data-testid="series-status-grid"
	>
		{#each items as item (item.issue.id)}
			{@const presentation = COLLECTION_STATUS_PRESENTATION[item.status]}
			<button
				type="button"
				class="group relative aspect-square min-w-0 rounded-lg font-bold transition-transform hover:scale-105 focus-visible:outline-2 focus-visible:outline-offset-2"
				style={cellStyle(item.status)}
				aria-label={`Heft #${item.issue.issue_number}: ${item.issue.title}. Status: ${presentation.label}. Details öffnen.`}
				title={`${item.issue.title} — ${presentation.label}`}
				onclick={(event) => onselect(item.issue, item.entry, event.currentTarget)}
				data-testid="series-status-cell"
				data-status={item.status}
			>
				<span class="text-sm sm:text-base">{item.issue.issue_number}</span>
				<span
					class="absolute right-0.5 top-0.5 flex h-4 min-w-4 items-center justify-center rounded bg-black/55 px-0.5 text-[8px] text-white"
					aria-hidden="true"
				>
					{presentation.abbreviation}
				</span>
			</button>
		{/each}
	</div>

	<div class="mt-5">
		<CollectionStatusLegend />
	</div>
</div>
