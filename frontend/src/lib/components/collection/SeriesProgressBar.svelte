<script lang="ts">
	interface Props {
		series_name: string;
		owned_count: number;
		total_count: number;
		duplicate_count?: number;
	}

	let { series_name, owned_count, total_count, duplicate_count = 0 }: Props = $props();

	const progress = $derived(total_count > 0 ? (owned_count / total_count) * 100 : 0);
	const displayPercent = $derived(progress.toFixed(1));
</script>

<div class="mb-4" data-testid="series-progress-bar">
	<div class="flex items-center justify-between mb-1">
		<span class="text-sm font-medium" style="color: var(--text-primary);">{series_name}</span>
		<span class="text-xs" style="color: var(--text-tertiary);">
			{owned_count} von {total_count} — {displayPercent}%
		</span>
	</div>
	<div
		class="w-full h-2 rounded-full overflow-hidden"
		style="background: var(--glass-border);"
		role="progressbar"
		aria-valuenow={progress}
		aria-valuemin={0}
		aria-valuemax={100}
		aria-label="{series_name}: {displayPercent}% gesammelt"
	>
		<div
			class="h-full rounded-full transition-all duration-700 ease-out"
			style="width: {progress}%; background: linear-gradient(90deg, var(--color-brand-500), var(--color-brand-700));"
		></div>
	</div>
	{#if duplicate_count > 0}
		<p class="text-[10px] mt-0.5" style="color: var(--color-status-duplicate);">
			{duplicate_count} Doppelte
		</p>
	{/if}
</div>
