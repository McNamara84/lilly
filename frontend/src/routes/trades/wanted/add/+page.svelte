<script lang="ts">
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { getAuthState } from '$lib/stores/auth.svelte';
	import { fetchSeries, type Series } from '$lib/api/series';
	import { addWantedBulk, fetchWantedCandidates, type WantedCandidate } from '$lib/api/trades';
	import { onDestroy } from 'svelte';
	import { SvelteSet } from 'svelte/reactivity';

	const auth = getAuthState();
	const PER_PAGE = 50;

	let seriesList = $state<Series[]>([]);
	let selectedSeriesSlug = $state('');
	let candidates = $state<WantedCandidate[]>([]);
	let selectedIds = new SvelteSet<number>();
	let search = $state('');
	let page = $state(1);
	let total = $state(0);
	let loadingSeries = $state(true);
	let loadingCandidates = $state(false);
	let saving = $state(false);
	let error = $state<string | null>(null);
	let announcement = $state('');
	let loaded = false;
	let candidateRequest: AbortController | null = null;

	const availableCandidates = $derived(candidates.filter((candidate) => !candidate.is_wanted));

	$effect(() => {
		if (!auth.isLoading && !auth.isAuthenticated) {
			void goto(resolve('/login'));
		} else if (auth.isAuthenticated && !loaded) {
			loaded = true;
			void loadSeries();
		}
	});

	async function loadSeries() {
		loadingSeries = true;
		error = null;
		try {
			seriesList = await fetchSeries();
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'Serien konnten nicht geladen werden.';
		} finally {
			loadingSeries = false;
		}
	}

	async function selectSeries(slug: string) {
		cancelCandidateRequest();
		selectedSeriesSlug = slug;
		selectedIds.clear();
		candidates = [];
		page = 1;
		total = 0;
		if (slug) await loadCandidates(1);
	}

	async function loadCandidates(nextPage: number) {
		if (!selectedSeriesSlug) return;
		cancelCandidateRequest();
		const controller = new AbortController();
		const requestedSeriesSlug = selectedSeriesSlug;
		candidateRequest = controller;
		loadingCandidates = true;
		error = null;
		try {
			const result = await fetchWantedCandidates(
				{
					series_slug: requestedSeriesSlug,
					q: search.trim() || undefined,
					page: nextPage,
					per_page: PER_PAGE
				},
				controller.signal
			);
			if (candidateRequest !== controller || selectedSeriesSlug !== requestedSeriesSlug) return;
			candidates = result.data;
			page = result.page;
			total = result.total;
			selectedIds.clear();
		} catch (cause) {
			if (controller.signal.aborted || candidateRequest !== controller) return;
			error =
				cause instanceof Error ? cause.message : 'Fehlende Hefte konnten nicht geladen werden.';
		} finally {
			if (candidateRequest === controller) {
				candidateRequest = null;
				loadingCandidates = false;
			}
		}
	}

	function cancelCandidateRequest() {
		candidateRequest?.abort();
		candidateRequest = null;
		loadingCandidates = false;
	}

	function toggleCandidate(issueId: number) {
		if (selectedIds.has(issueId)) selectedIds.delete(issueId);
		else selectedIds.add(issueId);
	}

	function toggleAllVisible() {
		const availableIds = availableCandidates.map((candidate) => candidate.issue_id);
		const everySelected =
			availableIds.length > 0 && availableIds.every((id) => selectedIds.has(id));
		for (const id of availableIds) {
			if (everySelected) selectedIds.delete(id);
			else selectedIds.add(id);
		}
	}

	async function addSelection() {
		if (selectedIds.size === 0) return;
		saving = true;
		error = null;
		try {
			const result = await addWantedBulk([...selectedIds]);
			const entryIds = new Map(
				[...result.created, ...result.unchanged].map((item) => [item.issue_id, item.entry_id])
			);
			const rejectedIds = new Set(result.rejected.map((item) => item.issue_id));
			candidates = candidates
				.filter((candidate) => !rejectedIds.has(candidate.issue_id))
				.map((candidate) => {
					const entryId = entryIds.get(candidate.issue_id);
					return entryId ? { ...candidate, is_wanted: true, wanted_entry_id: entryId } : candidate;
				});
			selectedIds.clear();
			announcement = `${result.created.length} Hefte zur Wunschliste hinzugefügt, ${result.unchanged.length} bereits vorhanden, ${result.rejected.length} abgelehnt.`;
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'Wünsche konnten nicht gespeichert werden.';
		} finally {
			saving = false;
		}
	}

	function handleSearch(event: SubmitEvent) {
		event.preventDefault();
		void loadCandidates(1);
	}

	onDestroy(cancelCandidateRequest);
</script>

<svelte:head>
	<title>Wünsche hinzufügen – LILLY</title>
</svelte:head>

<div class="min-h-[calc(100vh-3.5rem)] px-4 py-8 sm:px-6 lg:px-8">
	<header class="mb-6">
		<a
			href={resolve('/trades/wanted')}
			class="text-sm underline"
			style="color: var(--text-secondary);"
		>
			← Zurück zur Wunschliste
		</a>
		<h1 class="mt-3 text-2xl font-bold">Wünsche hinzufügen</h1>
		<p class="mt-1 text-sm" style="color: var(--text-secondary);">
			Wähle fehlende Hefte einzeln oder gesammelt aus.
		</p>
	</header>

	{#if error}
		<div class="glass-elevated mb-4 rounded-lg p-4" role="alert" data-testid="wanted-add-error">
			<p style="color: var(--color-error);">{error}</p>
		</div>
	{/if}

	<p class="sr-only" aria-live="polite">{announcement}</p>

	{#if loadingSeries}
		<p data-testid="series-loading">Lade Serien …</p>
	{:else if seriesList.length === 0}
		<p class="glass-elevated rounded-lg p-6" data-testid="series-empty">
			Noch keine aktiven Serien verfügbar.
		</p>
	{:else}
		<section class="glass-elevated mb-6 rounded-lg p-4" aria-labelledby="series-selection-title">
			<h2 id="series-selection-title" class="mb-3 text-lg font-semibold">Serie und Suche</h2>
			<div class="grid gap-4 md:grid-cols-[minmax(12rem,20rem)_1fr]">
				<div>
					<label for="wanted-series" class="mb-1 block text-sm">Serie</label>
					<select
						id="wanted-series"
						value={selectedSeriesSlug}
						onchange={(event) => selectSeries((event.currentTarget as HTMLSelectElement).value)}
						class="w-full rounded-lg p-2"
						style="background: var(--surface-raised); border: 1px solid var(--glass-border);"
						data-testid="series-select"
					>
						<option value="">Bitte wählen</option>
						{#each seriesList as series (series.id)}
							<option value={series.slug}>{series.name}</option>
						{/each}
					</select>
				</div>
				<form onsubmit={handleSearch}>
					<label for="wanted-search" class="mb-1 block text-sm">Titel oder Autor</label>
					<div class="flex gap-2">
						<input
							id="wanted-search"
							bind:value={search}
							disabled={!selectedSeriesSlug}
							maxlength="200"
							class="min-w-0 flex-1 rounded-lg p-2"
							style="background: var(--surface-raised); border: 1px solid var(--glass-border);"
						/>
						<button
							type="submit"
							disabled={!selectedSeriesSlug || loadingCandidates}
							class="rounded-lg px-4 py-2 disabled:opacity-50"
							style="background: var(--color-brand-500); color: #000;"
						>
							Suchen
						</button>
					</div>
				</form>
			</div>
		</section>
	{/if}

	{#if loadingCandidates}
		<p data-testid="candidates-loading">Lade fehlende Hefte …</p>
	{:else if selectedSeriesSlug && candidates.length === 0}
		<div class="glass-elevated rounded-lg p-8 text-center" data-testid="candidates-empty">
			Keine fehlenden Hefte für diese Auswahl gefunden.
		</div>
	{:else if candidates.length > 0}
		<div class="mb-4 flex flex-wrap items-center justify-between gap-3">
			<button
				type="button"
				onclick={toggleAllVisible}
				class="cursor-pointer rounded-lg px-3 py-2 text-sm"
				style="background: var(--glass); border: 1px solid var(--glass-border);"
				data-testid="toggle-all"
			>
				Alle verfügbaren auf dieser Seite auswählen
			</button>
			<button
				type="button"
				disabled={selectedIds.size === 0 || saving}
				onclick={addSelection}
				class="rounded-lg px-4 py-2 text-sm font-semibold disabled:opacity-50"
				style="background: var(--color-brand-500); color: #000;"
				data-testid="add-selection"
			>
				{saving ? 'Speichere …' : `${selectedIds.size} ausgewählte hinzufügen`}
			</button>
		</div>

		<ul class="grid gap-3 sm:grid-cols-2 lg:grid-cols-3" data-testid="candidate-list">
			{#each candidates as candidate (candidate.issue_id)}
				<li class="glass-elevated rounded-lg p-4" data-testid="candidate-item">
					<label class="flex cursor-pointer items-start gap-3">
						<input
							type="checkbox"
							checked={candidate.is_wanted || selectedIds.has(candidate.issue_id)}
							disabled={candidate.is_wanted}
							onchange={() => toggleCandidate(candidate.issue_id)}
							class="mt-1"
						/>
						<span>
							<span class="block text-xs" style="color: var(--text-tertiary);">
								{candidate.series_name} · #{candidate.issue_number}
							</span>
							<span class="block font-semibold">{candidate.title}</span>
							{#if candidate.is_wanted}
								<span class="mt-1 block text-xs" style="color: var(--color-status-wanted);">
									Bereits auf der Wunschliste
								</span>
							{/if}
						</span>
					</label>
				</li>
			{/each}
		</ul>

		{#if page > 1 || page * PER_PAGE < total}
			<nav class="mt-6 flex justify-center gap-3" aria-label="Kandidatenseiten">
				<button
					type="button"
					disabled={page <= 1}
					onclick={() => loadCandidates(page - 1)}
					class="rounded px-3 py-2 disabled:opacity-50"
				>
					Zurück
				</button>
				<button
					type="button"
					disabled={page * PER_PAGE >= total}
					onclick={() => loadCandidates(page + 1)}
					class="rounded px-3 py-2 disabled:opacity-50"
				>
					Weiter
				</button>
			</nav>
		{/if}
	{/if}
</div>
