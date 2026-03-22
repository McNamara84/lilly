<script lang="ts">
	import type { CollectionEntry } from '$lib/api/collection';

	interface Props {
		entry: CollectionEntry;
		size?: 'sm' | 'md' | 'lg';
		interactive?: boolean;
		onclick?: () => void;
	}

	let { entry, size = 'md', interactive = true, onclick }: Props = $props();

	const sizeClasses = $derived(size === 'sm' ? 'w-24' : size === 'lg' ? 'w-48' : 'w-36');

	const coverAlt = $derived(
		`Cover von ${entry.series_name} #${entry.issue_number}: ${entry.title}`
	);

	const isMissing = $derived(entry.status === 'missing');
</script>

<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
<article
	class="relative rounded-lg overflow-hidden {sizeClasses} {interactive ? 'cursor-pointer' : ''}"
	class:opacity-40={isMissing}
	style={isMissing ? 'border: 2px dashed var(--glass-border);' : ''}
	role={interactive ? 'button' : undefined}
	tabindex={interactive ? 0 : undefined}
	onclick={interactive ? onclick : undefined}
	onkeydown={interactive
		? (e: KeyboardEvent) => {
				if (e.key === 'Enter' || e.key === ' ') {
					e.preventDefault();
					onclick?.();
				}
			}
		: undefined}
	aria-label="{entry.title} — #{entry.issue_number}{entry.status !== 'missing'
		? `, ${entry.status}, ${entry.condition_grade}`
		: ', fehlend'}"
	data-testid="cover-card"
>
	<!-- Cover image -->
	<div class="aspect-[2/3] bg-gray-800 rounded-lg overflow-hidden">
		{#if entry.cover_url || entry.cover_local_path}
			<img
				src={entry.cover_local_path ?? entry.cover_url}
				alt={coverAlt}
				class="w-full h-full object-cover"
				loading="lazy"
			/>
		{:else}
			<div
				class="w-full h-full flex items-center justify-center text-sm"
				style="color: var(--text-tertiary);"
			>
				#{entry.issue_number}
			</div>
		{/if}
	</div>

	<!-- Issue number pill (top-left) -->
	<span
		class="absolute top-1 left-1 px-1.5 py-0.5 text-[10px] font-bold rounded"
		style="background: var(--glass); backdrop-filter: blur(8px); color: var(--text-primary);"
	>
		#{entry.issue_number}
	</span>

	<!-- Condition badge (top-right) -->
	{#if entry.condition_grade && !isMissing}
		<span
			class="absolute top-1 right-1 px-1.5 py-0.5 text-[10px] font-bold rounded"
			style="background-color: var(--color-condition-{entry.condition_grade.toLowerCase()}); color: #000;"
		>
			{entry.condition_grade}
		</span>
	{/if}

	<!-- Status indicator (bottom) -->
	{#if entry.status === 'owned'}
		<div
			class="absolute bottom-0 left-0 right-0 h-0.5"
			style="background: var(--color-status-owned); box-shadow: 0 0 8px var(--color-status-owned);"
		></div>
	{:else if entry.status === 'duplicate'}
		<span
			class="absolute bottom-1 right-1 px-1 py-0.5 text-[8px] font-bold rounded"
			style="background-color: var(--color-status-duplicate); color: #fff;"
		>
			Doppelt
		</span>
	{:else if entry.status === 'wanted'}
		<span
			class="absolute bottom-1 right-1 w-2 h-2 rounded-full animate-pulse"
			style="background-color: var(--color-status-wanted);"
		></span>
	{/if}

	<!-- Title below cover -->
	<p
		class="mt-1 text-xs truncate text-center"
		style="color: var(--text-secondary);"
		title={entry.title}
	>
		{entry.title}
	</p>
</article>
