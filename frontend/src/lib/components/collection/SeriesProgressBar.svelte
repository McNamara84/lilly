<script lang="ts">
	interface Props {
		series_name: string;
		owned_count: number;
		total_count: number | null;
		progress_percent?: number | null;
		duplicate_count?: number;
	}

	let {
		series_name,
		owned_count,
		total_count,
		progress_percent = undefined,
		duplicate_count = 0
	}: Props = $props();

	const hasPositiveTotal = $derived(total_count !== null && total_count > 0);
	const progress = $derived(
		hasPositiveTotal
			? Math.min(
					100,
					Math.max(0, progress_percent ?? (owned_count / (total_count as number)) * 100)
				)
			: null
	);
	const displayPercent = $derived(progress === null ? null : progress.toFixed(1));
</script>

<div class="mb-4" data-testid="series-progress-bar">
	<div class="flex items-center justify-between gap-3 mb-1">
		<span class="text-sm font-medium" style="color: var(--text-primary);">{series_name}</span>
		<span class="text-xs text-right" style="color: var(--text-tertiary);">
			{#if total_count === null}
				{owned_count} gesammelt — Gesamtzahl unbekannt
			{:else if total_count === 0}
				Noch keine Hefte verfügbar
			{:else}
				{owned_count} von {total_count} — {displayPercent}%
			{/if}
		</span>
	</div>

	{#if progress !== null && displayPercent !== null}
		<div
			class="w-full h-2 rounded-full overflow-hidden"
			style="background: var(--glass-border);"
			role="progressbar"
			aria-valuenow={progress}
			aria-valuemin={0}
			aria-valuemax={100}
			aria-label="{series_name}: {owned_count} von {total_count} Heften, {displayPercent}% gesammelt"
		>
			<div
				class="h-full rounded-full transition-all duration-700 ease-out"
				style="width: {progress}%; background: linear-gradient(90deg, var(--color-brand-500), var(--color-brand-700));"
			></div>
		</div>
	{:else}
		<p class="sr-only" data-testid="progress-unavailable">
			{series_name}: Kein prozentualer Fortschritt verfügbar
		</p>
	{/if}

	{#if duplicate_count > 0}
		<p class="text-[10px] mt-0.5" style="color: var(--color-status-duplicate);">
			{duplicate_count} Doppelte
		</p>
	{/if}
</div>
