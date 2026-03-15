<script lang="ts">
	import { fetchSeries, type Series } from '$lib/api/series';
	import { resolve } from '$app/paths';

	let seriesList = $state<Series[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);

	async function loadSeries() {
		loading = true;
		try {
			seriesList = await fetchSeries();
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load series';
		} finally {
			loading = false;
		}
	}

	$effect(() => {
		loadSeries();
	});
</script>

<svelte:head>
	<title>Serien – LILLY</title>
</svelte:head>

<div class="min-h-[calc(100vh-3.5rem)] px-4 py-8 sm:px-6 lg:px-8">
	<h1 class="text-2xl font-bold mb-6" style="color: var(--text-primary);" data-testid="series-title">
		Serien
	</h1>

	{#if loading}
		<p style="color: var(--text-secondary);" data-testid="loading-indicator">Lade Serien...</p>
	{:else if error}
		<div
			class="p-4 rounded-lg mb-4"
			style="background-color: var(--color-error-100); color: var(--color-error-700);"
			role="alert"
			data-testid="error-message"
		>
			{error}
		</div>
	{:else if seriesList.length === 0}
		<p style="color: var(--text-secondary);" data-testid="empty-state">
			Noch keine Serien verfügbar.
		</p>
	{:else}
		<div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-6" data-testid="series-grid">
			{#each seriesList as series (series.id)}
				<a
					href={resolve(`/series/${series.slug}`)}
					class="glass-elevated rounded-lg p-6 block transition-transform hover:scale-[1.02]"
					data-testid="series-card"
				>
					<h2 class="text-lg font-semibold mb-2" style="color: var(--text-primary);">
						{series.name}
					</h2>
					<div class="flex flex-wrap gap-2 mb-3">
						{#if series.genre}
							<span
								class="text-xs px-2 py-0.5 rounded"
								style="background-color: var(--color-brand-100); color: var(--color-brand-700);"
							>
								{series.genre}
							</span>
						{/if}
						<span
							class="text-xs px-2 py-0.5 rounded"
							style="background-color: var(--surface-raised); color: var(--text-secondary);"
						>
							{series.status}
						</span>
					</div>
					{#if series.publisher}
						<p class="text-sm" style="color: var(--text-secondary);">
							{series.publisher}
						</p>
					{/if}
					{#if series.total_issues}
						<p class="text-sm mt-1" style="color: var(--text-tertiary);">
							{series.total_issues} Hefte
						</p>
					{/if}
				</a>
			{/each}
		</div>
	{/if}
</div>
