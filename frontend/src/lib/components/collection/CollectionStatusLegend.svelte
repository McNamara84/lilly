<script lang="ts">
	import { COLLECTION_STATUS_PRESENTATION } from '$lib/collection/series-grid';
	import type { CollectionStatus } from '$lib/api/collection';

	const STATUSES: CollectionStatus[] = ['owned', 'duplicate', 'wanted', 'missing'];
</script>

<ul
	class="flex flex-wrap gap-x-4 gap-y-2 text-xs"
	style="color: var(--text-secondary);"
	aria-label="Legende der Sammlungszustände"
	data-testid="collection-status-legend"
>
	{#each STATUSES as status (status)}
		<li class="flex items-center gap-1.5" data-testid={`legend-${status}`}>
			<span
				class="inline-flex h-5 w-5 items-center justify-center rounded text-[9px] font-bold"
				class:border={status === 'missing'}
				class:border-dashed={status === 'missing'}
				style={status === 'missing'
					? 'border-color: var(--glass-border); color: var(--text-secondary);'
					: `background: var(--color-status-${status}); color: #000;`}
				aria-hidden="true"
			>
				{COLLECTION_STATUS_PRESENTATION[status].abbreviation}
			</span>
			{COLLECTION_STATUS_PRESENTATION[status].label}
		</li>
	{/each}
</ul>
