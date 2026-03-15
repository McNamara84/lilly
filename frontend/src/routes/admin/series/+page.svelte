<script lang="ts">
	import {
		fetchAllSeries,
		activateSeries,
		deactivateSeries,
		type SeriesAdmin
	} from '$lib/api/admin';

	let seriesList = $state<SeriesAdmin[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);

	async function loadSeries() {
		loading = true;
		error = null;
		try {
			seriesList = await fetchAllSeries();
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load series';
		} finally {
			loading = false;
		}
	}

	async function toggleActive(series: SeriesAdmin) {
		try {
			if (series.active) {
				await deactivateSeries(series.slug);
			} else {
				await activateSeries(series.slug);
			}
			await loadSeries();
		} catch (e) {
			error = e instanceof Error ? e.message : 'Operation failed';
		}
	}

	$effect(() => {
		loadSeries();
	});
</script>

<svelte:head>
	<title>Serien-Verwaltung – LILLY Admin</title>
</svelte:head>

<h1 class="text-2xl font-bold mb-6" style="color: var(--text-primary);" data-testid="admin-series-title">
	Serien-Verwaltung
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
		Noch keine Serien vorhanden. Starte einen Import, um Serien hinzuzufügen.
	</p>
{:else}
	<div class="overflow-x-auto">
		<table class="w-full text-sm" data-testid="series-table">
			<thead>
				<tr style="border-bottom: 1px solid var(--border-default);">
					<th class="text-left py-3 px-2" style="color: var(--text-secondary);">Name</th>
					<th class="text-left py-3 px-2" style="color: var(--text-secondary);">Status</th>
					<th class="text-center py-3 px-2" style="color: var(--text-secondary);">Hefte</th>
					<th class="text-center py-3 px-2" style="color: var(--text-secondary);">Aktiv</th>
					<th class="text-right py-3 px-2" style="color: var(--text-secondary);">Aktion</th>
				</tr>
			</thead>
			<tbody>
				{#each seriesList as series (series.id)}
					<tr
						style="border-bottom: 1px solid var(--border-default);"
						data-testid="series-row"
					>
						<td class="py-3 px-2 font-medium" style="color: var(--text-primary);">
							{series.name}
						</td>
						<td class="py-3 px-2">
							<span
								class="inline-block px-2 py-0.5 rounded text-xs font-medium"
								style="background-color: var(--color-brand-100); color: var(--color-brand-700);"
							>
								{series.status}
							</span>
						</td>
						<td class="py-3 px-2 text-center" style="color: var(--text-secondary);">
							{series.total_issues ?? '–'}
						</td>
						<td class="py-3 px-2 text-center">
							{#if series.active}
								<span
									class="inline-block w-3 h-3 rounded-full"
									style="background-color: var(--color-success-500);"
									aria-label="Aktiv"
								></span>
							{:else}
								<span
									class="inline-block w-3 h-3 rounded-full"
									style="background-color: var(--text-tertiary);"
									aria-label="Inaktiv"
								></span>
							{/if}
						</td>
						<td class="py-3 px-2 text-right">
							<button
								onclick={() => toggleActive(series)}
								class="text-xs px-3 py-1.5 rounded-lg cursor-pointer transition-colors"
								style="background-color: var(--surface-raised); color: var(--text-secondary);"
								data-testid="toggle-active-button"
							>
								{series.active ? 'Deaktivieren' : 'Aktivieren'}
							</button>
						</td>
					</tr>
				{/each}
			</tbody>
		</table>
	</div>
{/if}
