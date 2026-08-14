<script lang="ts">
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import {
		addToCollection,
		deleteCollectionEntry,
		fetchAllCollectionEntries,
		updateCollectionEntry,
		type CollectionEntry,
		type PersistedCollectionStatus
	} from '$lib/api/collection';
	import { fetchAllSeriesIssues, fetchSeries, type Issue, type Series } from '$lib/api/series';
	import type { ConditionGrade } from '$lib/collection/conditions';
	import IssueDetailSheet from '$lib/components/collection/IssueDetailSheet.svelte';
	import SeriesStatusGrid from '$lib/components/collection/SeriesStatusGrid.svelte';
	import { getAuthState } from '$lib/stores/auth.svelte';

	const auth = getAuthState();

	let seriesList = $state<Series[]>([]);
	let selectedSeries = $state<Series | null>(null);
	let issues = $state<Issue[]>([]);
	let entries = $state<CollectionEntry[]>([]);
	let selectedIssue = $state<Issue | null>(null);
	let selectedEntry = $state<CollectionEntry | null>(null);
	let selectedEntries = $state<CollectionEntry[]>([]);
	let loading = $state(true);
	let gridLoading = $state(false);
	let error = $state<string | null>(null);
	let sheetError = $state<string | null>(null);
	let toast = $state<string | null>(null);
	let toastTimeoutId: ReturnType<typeof setTimeout> | null = null;
	let detailTrigger: HTMLButtonElement | null = null;
	let seriesRequested = false;

	$effect(() => {
		if (!auth.isLoading && !auth.isAuthenticated) {
			void goto(resolve('/login'));
		} else if (auth.isAuthenticated && !seriesRequested) {
			seriesRequested = true;
			void loadSeries();
		}
	});

	$effect(() => {
		return () => {
			if (toastTimeoutId !== null) clearTimeout(toastTimeoutId);
		};
	});

	async function loadSeries() {
		loading = true;
		error = null;
		try {
			seriesList = await fetchSeries();
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'Serien konnten nicht geladen werden.';
		} finally {
			loading = false;
		}
	}

	async function selectSeries(series: Series) {
		selectedSeries = series;
		gridLoading = true;
		error = null;
		try {
			[issues, entries] = await Promise.all([
				fetchAllSeriesIssues(series.slug),
				fetchAllCollectionEntries(series.slug)
			]);
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'Hefte konnten nicht geladen werden.';
		} finally {
			gridLoading = false;
		}
	}

	function returnToSeriesList() {
		selectedSeries = null;
		issues = [];
		entries = [];
		closeSheet(false);
	}

	function openDetails(issue: Issue, entry: CollectionEntry | null, trigger: HTMLButtonElement) {
		sheetError = null;
		selectedIssue = issue;
		selectedEntries = entries.filter((candidate) => candidate.issue_id === issue.id);
		selectedEntry = entry;
		detailTrigger = trigger;
	}

	function closeSheet(restoreFocus = true) {
		selectedIssue = null;
		selectedEntry = null;
		selectedEntries = [];
		sheetError = null;
		if (restoreFocus && detailTrigger) {
			const trigger = detailTrigger;
			queueMicrotask(() => trigger.focus());
		}
		detailTrigger = null;
	}

	function showToast(message: string) {
		if (toastTimeoutId !== null) clearTimeout(toastTimeoutId);
		toast = message;
		toastTimeoutId = setTimeout(() => {
			toast = null;
			toastTimeoutId = null;
		}, 2500);
	}

	async function handleSave(data: {
		issue_id: number;
		condition_grade?: ConditionGrade;
		status: PersistedCollectionStatus;
		notes: string;
		edition_label: string;
	}) {
		sheetError = null;
		try {
			if (selectedEntry) {
				const updated = await updateCollectionEntry(selectedEntry.id, {
					condition_grade: data.condition_grade,
					status: data.status,
					notes: data.notes,
					edition_label: data.edition_label
				});
				entries = entries.map((entry) => (entry.id === updated.id ? updated : entry));
				showToast(`Heft #${updated.issue_number} aktualisiert`);
			} else {
				const created = await addToCollection(data);
				entries = [...entries, created];
				showToast(`Heft #${created.issue_number} hinzugefügt`);
			}
			closeSheet();
		} catch (cause) {
			sheetError =
				cause instanceof Error ? cause.message : 'Eintrag konnte nicht gespeichert werden.';
		}
	}

	async function handleDelete() {
		if (!selectedEntry) return;
		sheetError = null;
		try {
			await deleteCollectionEntry(selectedEntry.id);
			const removedIssueNumber = selectedEntry.issue_number;
			entries = entries.filter((entry) => entry.id !== selectedEntry?.id);
			closeSheet();
			showToast(`Heft #${removedIssueNumber} entfernt`);
		} catch (cause) {
			sheetError = cause instanceof Error ? cause.message : 'Eintrag konnte nicht entfernt werden.';
		}
	}
</script>

<svelte:head>
	<title>Serienraster – LILLY</title>
</svelte:head>

<div class="min-h-[calc(100vh-3.5rem)] px-4 py-8 sm:px-6 lg:px-8">
	<div class="mb-6 flex items-center gap-4">
		{#if selectedSeries}
			<button
				type="button"
				onclick={returnToSeriesList}
				class="cursor-pointer text-sm"
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
		<div role="alert" class="glass-elevated mb-4 rounded-lg p-4" data-testid="error-message">
			<p style="color: var(--color-error);">{error}</p>
		</div>
	{/if}

	{#if !selectedSeries}
		{#if loading}
			<p data-testid="loading-indicator">Lade Serien …</p>
		{:else if seriesList.length === 0}
			<p style="color: var(--text-secondary);" data-testid="empty-state">
				Noch keine Serien verfügbar.
			</p>
		{:else}
			<div
				class="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3"
				data-testid="series-selector"
			>
				{#each seriesList as series (series.id)}
					<button
						type="button"
						class="glass-elevated cursor-pointer rounded-lg p-6 text-left transition-all hover:scale-[1.02]"
						onclick={() => selectSeries(series)}
						data-testid="series-card"
					>
						<h2 class="text-lg font-semibold" style="color: var(--text-primary);">{series.name}</h2>
						{#if series.total_issues}
							<p class="mt-1 text-sm" style="color: var(--text-secondary);">
								{series.total_issues} Hefte
							</p>
						{/if}
					</button>
				{/each}
			</div>
		{/if}
	{:else if gridLoading}
		<p data-testid="loading-indicator">Lade Hefte …</p>
	{:else if issues.length === 0}
		<p style="color: var(--text-secondary);" data-testid="empty-state">
			Keine Hefte in dieser Serie.
		</p>
	{:else}
		<p class="mb-4 text-sm" style="color: var(--text-secondary);">
			Wähle ein Heft, um Status, Zustand und persönliche Notiz zu bearbeiten.
		</p>
		<SeriesStatusGrid {issues} {entries} onselect={openDetails} />
	{/if}
</div>

<IssueDetailSheet
	issue={selectedIssue}
	collection_entry={selectedEntry}
	collection_entries={selectedEntries}
	onselectentry={(entry) => (selectedEntry = entry)}
	onaddcopy={() => (selectedEntry = null)}
	onclose={closeSheet}
	onsave={handleSave}
	ondelete={handleDelete}
	error={sheetError}
/>

{#if toast}
	<div
		class="fixed bottom-6 left-1/2 z-50 -translate-x-1/2 rounded-lg px-4 py-2 text-sm font-medium"
		style="background: var(--surface-raised); border: 1px solid var(--glass-border); color: var(--text-primary);"
		role="status"
		data-testid="toast"
	>
		{toast}
	</div>
{/if}
