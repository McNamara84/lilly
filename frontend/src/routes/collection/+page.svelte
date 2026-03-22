<script lang="ts">
	import { getAuthState } from '$lib/stores/auth.svelte';
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import {
		fetchCollection,
		updateCollectionEntry,
		deleteCollectionEntry,
		type CollectionEntry,
		type CollectionQueryParams
	} from '$lib/api/collection';
	import { fetchSeries } from '$lib/api/series';
	import CollectionFilterBar from '$lib/components/collection/CollectionFilterBar.svelte';
	import CoverGrid from '$lib/components/collection/CoverGrid.svelte';
	import IssueDetailSheet from '$lib/components/collection/IssueDetailSheet.svelte';
	import { fetchIssue, type Issue } from '$lib/api/series';

	const auth = getAuthState();

	let entries = $state<CollectionEntry[]>([]);
	let total = $state(0);
	let loading = $state(true);
	let error = $state<string | null>(null);
	let seriesOptions = $state<{ slug: string; name: string }[]>([]);
	let currentParams = $state<CollectionQueryParams>({});

	// Detail sheet state
	let selectedIssue = $state<Issue | null>(null);
	let selectedEntry = $state<CollectionEntry | null>(null);

	$effect(() => {
		if (!auth.isLoading && !auth.isAuthenticated) {
			goto(resolve('/login'));
		}
	});

	$effect(() => {
		if (auth.isAuthenticated) {
			loadSeries();
			loadCollection({});
		}
	});

	async function loadSeries() {
		try {
			const all = await fetchSeries();
			seriesOptions = all.map((s) => ({ slug: s.slug, name: s.name }));
		} catch {
			// Non-critical; filter dropdown will be empty
		}
	}

	async function loadCollection(params: CollectionQueryParams) {
		loading = true;
		error = null;
		currentParams = params;
		try {
			const result = await fetchCollection(params);
			entries = result.data;
			total = result.total;
		} catch (e) {
			error = e instanceof Error ? e.message : 'Fehler beim Laden der Sammlung';
		} finally {
			loading = false;
		}
	}

	function handleFilterChange(params: CollectionQueryParams) {
		loadCollection(params);
	}

	async function handleSelect(entry: CollectionEntry) {
		try {
			const issue = await fetchIssue(entry.issue_id);
			selectedIssue = issue;
			selectedEntry = entry.id > 0 ? entry : null;
		} catch {
			// Could not load issue details
		}
	}

	function closeSheet() {
		selectedIssue = null;
		selectedEntry = null;
	}

	async function handleSave(data: {
		issue_id: number;
		condition_grade: string;
		status?: string;
		notes?: string;
	}) {
		try {
			if (selectedEntry && selectedEntry.id > 0) {
				await updateCollectionEntry(selectedEntry.id, {
					condition_grade: data.condition_grade,
					status: data.status,
					notes: data.notes
				});
			}
			closeSheet();
			loadCollection(currentParams);
		} catch {
			// Error handling could be improved with toast
		}
	}

	async function handleDelete() {
		if (!selectedEntry || selectedEntry.id <= 0) return;
		try {
			await deleteCollectionEntry(selectedEntry.id);
			closeSheet();
			loadCollection(currentParams);
		} catch {
			// Error handling could be improved with toast
		}
	}
</script>

<svelte:head>
	<title>Meine Sammlung – LILLY</title>
</svelte:head>

<div class="min-h-[calc(100vh-3.5rem)]">
	<div class="px-4 py-8 sm:px-6 lg:px-8">
		<div class="flex items-center justify-between mb-6">
			<h1
				class="text-2xl font-bold"
				style="color: var(--text-primary);"
				data-testid="collection-title"
			>
				Meine Sammlung
			</h1>
			<a
				href={resolve('/collection/add')}
				class="rounded-lg px-4 py-2 text-sm font-semibold"
				style="background: var(--color-brand-500); color: #000;"
				data-testid="collection-fab"
			>
				+ Hinzufügen
			</a>
		</div>

		{#if error}
			<div
				class="glass-elevated p-4 rounded-lg mb-4"
				role="alert"
				style="border-color: var(--color-error);"
				data-testid="collection-error"
			>
				<p style="color: var(--color-error);">{error}</p>
			</div>
		{/if}
	</div>

	<CollectionFilterBar series_options={seriesOptions} onfilterchange={handleFilterChange} />

	<div class="px-4 py-6 sm:px-6 lg:px-8">
		<CoverGrid items={entries} {loading} onselect={handleSelect} />

		{#if !loading && total > 0}
			<p class="text-center text-xs mt-4" style="color: var(--text-tertiary);">
				{entries.length} von {total} Einträgen
			</p>
		{/if}
	</div>
</div>

<IssueDetailSheet
	issue={selectedIssue}
	collection_entry={selectedEntry}
	onclose={closeSheet}
	onsave={handleSave}
	ondelete={handleDelete}
/>
