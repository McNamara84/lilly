<script lang="ts">
	import { getAuthState } from '$lib/stores/auth.svelte';
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { fetchSeries, fetchAllSeriesIssues, type Series, type Issue } from '$lib/api/series';
	import {
		fetchAllCollectionEntries,
		addToCollection,
		deleteCollectionEntry,
		type CollectionEntry
	} from '$lib/api/collection';
	import { SvelteMap } from 'svelte/reactivity';

	const auth = getAuthState();

	let seriesList = $state<Series[]>([]);
	let selectedSeries = $state<Series | null>(null);
	let issues = $state<Issue[]>([]);
	let collectionEntries = new SvelteMap<number, CollectionEntry>();
	let loading = $state(true);
	let gridLoading = $state(false);
	let error = $state<string | null>(null);
	let toast = $state<string | null>(null);
	let toastTimeoutId: ReturnType<typeof setTimeout> | null = null;

	// Clean up toast timer on component destroy
	$effect(() => {
		return () => {
			if (toastTimeoutId !== null) {
				clearTimeout(toastTimeoutId);
			}
		};
	});

	$effect(() => {
		if (!auth.isLoading && !auth.isAuthenticated) {
			goto(resolve('/login'));
		}
	});

	$effect(() => {
		if (auth.isAuthenticated) {
			loadSeries();
		}
	});

	async function loadSeries() {
		loading = true;
		try {
			seriesList = await fetchSeries();
		} catch (e) {
			error = e instanceof Error ? e.message : 'Fehler beim Laden';
		} finally {
			loading = false;
		}
	}

	async function selectSeries(series: Series) {
		selectedSeries = series;
		gridLoading = true;
		error = null;
		try {
			// Load all issues and the user's collection for this series
			const [allIssues, collectionResult] = await Promise.all([
				fetchAllSeriesIssues(series.slug),
				fetchAllCollectionEntries(series.slug)
			]);
			issues = allIssues;
			collectionEntries.clear();
			for (const entry of collectionResult) {
				collectionEntries.set(entry.issue_id, entry);
			}
		} catch (e) {
			error = e instanceof Error ? e.message : 'Fehler beim Laden der Hefte';
		} finally {
			gridLoading = false;
		}
	}

	function showToast(msg: string) {
		if (toastTimeoutId !== null) {
			clearTimeout(toastTimeoutId);
		}
		toast = msg;
		toastTimeoutId = setTimeout(() => {
			toast = null;
			toastTimeoutId = null;
		}, 2500);
	}

	async function toggleIssue(issue: Issue) {
		const existing = collectionEntries.get(issue.id);
		if (existing) {
			// Remove from collection
			try {
				await deleteCollectionEntry(existing.id);
				collectionEntries.delete(issue.id);
				showToast(`Heft #${issue.issue_number} entfernt`);
			} catch {
				showToast('Fehler beim Entfernen');
			}
		} else {
			// Add to collection (quick-add with Z2 default)
			try {
				const entry = await addToCollection({
					issue_id: issue.id,
					condition_grade: 'Z2',
					status: 'owned'
				});
				collectionEntries.set(issue.id, entry);
				showToast(`Heft #${issue.issue_number} hinzugefügt ✓`);
			} catch (e) {
				showToast(e instanceof Error ? e.message : 'Fehler beim Hinzufügen');
			}
		}
	}

	function getStatusColor(issueId: number): string {
		const entry = collectionEntries.get(issueId);
		if (!entry) return 'var(--glass-border)';
		switch (entry.status) {
			case 'owned':
				return 'var(--color-status-owned)';
			case 'duplicate':
				return 'var(--color-status-duplicate)';
			case 'wanted':
				return 'var(--color-status-wanted)';
			default:
				return 'var(--glass-border)';
		}
	}
</script>

<svelte:head>
	<title>Hefte hinzufügen – LILLY</title>
</svelte:head>

<div class="min-h-[calc(100vh-3.5rem)] px-4 py-8 sm:px-6 lg:px-8">
	<div class="flex items-center gap-4 mb-6">
		{#if selectedSeries}
			<button
				onclick={() => {
					selectedSeries = null;
					issues = [];
					collectionEntries.clear();
				}}
				class="text-sm cursor-pointer"
				style="color: var(--text-secondary);"
				data-testid="back-button"
			>
				← Zurück
			</button>
		{/if}
		<h1 class="text-2xl font-bold" style="color: var(--text-primary);" data-testid="add-title">
			{selectedSeries ? selectedSeries.name : 'Serie wählen'}
		</h1>
	</div>

	{#if error}
		<div role="alert" class="glass-elevated p-4 rounded-lg mb-4" data-testid="error-message">
			<p style="color: var(--color-error);">{error}</p>
		</div>
	{/if}

	{#if !selectedSeries}
		<!-- Series selection -->
		{#if loading}
			<p data-testid="loading-indicator">Lade Serien...</p>
		{:else if seriesList.length === 0}
			<p style="color: var(--text-secondary);" data-testid="empty-state">
				Noch keine Serien verfügbar.
			</p>
		{:else}
			<div
				class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4"
				data-testid="series-selector"
			>
				{#each seriesList as series (series.id)}
					<button
						class="glass-elevated rounded-lg p-6 text-left cursor-pointer transition-all hover:scale-[1.02]"
						onclick={() => selectSeries(series)}
						data-testid="series-card"
					>
						<h2 class="text-lg font-semibold" style="color: var(--text-primary);">
							{series.name}
						</h2>
						{#if series.total_issues}
							<p class="text-sm mt-1" style="color: var(--text-secondary);">
								{series.total_issues} Hefte
							</p>
						{/if}
					</button>
				{/each}
			</div>
		{/if}
	{:else}
		<!-- Number grid -->
		{#if gridLoading}
			<p data-testid="loading-indicator">Lade Hefte...</p>
		{:else if issues.length === 0}
			<p style="color: var(--text-secondary);" data-testid="empty-state">
				Keine Hefte in dieser Serie.
			</p>
		{:else}
			<div
				class="grid gap-2"
				style="grid-template-columns: repeat(auto-fill, minmax(48px, 1fr));"
				data-testid="number-grid"
			>
				{#each issues as issue (issue.id)}
					{@const inCollection = collectionEntries.has(issue.id)}
					<button
						class="aspect-square rounded-lg flex items-center justify-center text-sm font-bold transition-all cursor-pointer"
						style="background: {getStatusColor(issue.id)}; color: {inCollection
							? '#000'
							: 'var(--text-secondary)'}; border: {inCollection
							? 'none'
							: '1px solid var(--glass-border)'};"
						onclick={() => toggleIssue(issue)}
						title="{issue.title} — #{issue.issue_number}"
						aria-label="Heft #{issue.issue_number}: {issue.title}{inCollection
							? ' (in Sammlung)'
							: ''}"
						data-testid="number-cell"
					>
						{issue.issue_number}
					</button>
				{/each}
			</div>

			<div class="mt-4 flex gap-4 text-xs" style="color: var(--text-tertiary);">
				<span>
					<span
						class="inline-block w-3 h-3 rounded mr-1"
						style="background: var(--color-status-owned);"
					></span>Vorhanden
				</span>
				<span>
					<span
						class="inline-block w-3 h-3 rounded mr-1"
						style="background: var(--color-status-duplicate);"
					></span>Doppelt
				</span>
				<span>
					<span
						class="inline-block w-3 h-3 rounded mr-1"
						style="background: var(--color-status-wanted);"
					></span>Gesucht
				</span>
			</div>
		{/if}
	{/if}
</div>

<!-- Toast notification -->
{#if toast}
	<div
		class="fixed bottom-6 left-1/2 -translate-x-1/2 z-50 rounded-lg px-4 py-2 text-sm font-medium"
		style="background: var(--surface-raised); border: 1px solid var(--glass-border); color: var(--text-primary);"
		role="status"
		data-testid="toast"
	>
		{toast}
	</div>
{/if}
