<script lang="ts">
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { getAuthState } from '$lib/stores/auth.svelte';
	import { updateCollectionEntry } from '$lib/api/collection';
	import {
		deleteWantedEntry,
		fetchTradeOffers,
		fetchWantedEntries,
		type TradeOffer,
		type WantedEntry
	} from '$lib/api/trades';

	const auth = getAuthState();
	const PER_PAGE = 24;

	let activeTab = $state<'offers' | 'wanted'>('offers');
	let offers = $state<TradeOffer[]>([]);
	let wanted = $state<WantedEntry[]>([]);
	let offersPage = $state(1);
	let wantedPage = $state(1);
	let offersTotal = $state(0);
	let wantedTotal = $state(0);
	let loadingOffers = $state(true);
	let loadingWanted = $state(true);
	let offersError = $state<string | null>(null);
	let wantedError = $state<string | null>(null);
	let announcement = $state('');
	let loaded = false;

	$effect(() => {
		if (!auth.isLoading && !auth.isAuthenticated) {
			void goto(resolve('/login'));
		} else if (auth.isAuthenticated && !loaded) {
			loaded = true;
			void Promise.all([loadOffers(1), loadWanted(1)]);
		}
	});

	async function loadOffers(page: number) {
		loadingOffers = true;
		offersError = null;
		try {
			const result = await fetchTradeOffers({ page, per_page: PER_PAGE });
			offers = result.data;
			offersPage = result.page;
			offersTotal = result.total;
		} catch (cause) {
			offersError =
				cause instanceof Error ? cause.message : 'Tauschangebote konnten nicht geladen werden.';
		} finally {
			loadingOffers = false;
		}
	}

	async function loadWanted(page: number) {
		loadingWanted = true;
		wantedError = null;
		try {
			const result = await fetchWantedEntries({ page, per_page: PER_PAGE });
			wanted = result.data;
			wantedPage = result.page;
			wantedTotal = result.total;
		} catch (cause) {
			wantedError =
				cause instanceof Error ? cause.message : 'Wunschliste konnte nicht geladen werden.';
		} finally {
			loadingWanted = false;
		}
	}

	async function deactivateOffer(offer: TradeOffer) {
		offersError = null;
		try {
			await updateCollectionEntry(offer.entry_id, { status: 'owned' });
			offers = offers.filter((candidate) => candidate.entry_id !== offer.entry_id);
			offersTotal = Math.max(0, offersTotal - 1);
			announcement = `Heft #${offer.issue_number} ist nicht mehr tauschbar.`;
		} catch (cause) {
			offersError =
				cause instanceof Error ? cause.message : 'Angebot konnte nicht entfernt werden.';
		}
	}

	async function removeWanted(entry: WantedEntry) {
		wantedError = null;
		try {
			await deleteWantedEntry(entry.entry_id);
			wanted = wanted.filter((candidate) => candidate.entry_id !== entry.entry_id);
			wantedTotal = Math.max(0, wantedTotal - 1);
			announcement = `Heft #${entry.issue_number} wurde von der Wunschliste entfernt.`;
		} catch (cause) {
			wantedError = cause instanceof Error ? cause.message : 'Wunsch konnte nicht entfernt werden.';
		}
	}

	function coverSource(entry: TradeOffer | WantedEntry): string | null {
		return entry.cover_local_path ?? entry.cover_url;
	}
</script>

<svelte:head>
	<title>Tauschlisten – LILLY</title>
</svelte:head>

<div class="min-h-[calc(100vh-3.5rem)] px-4 py-8 sm:px-6 lg:px-8">
	<header class="mb-6 flex flex-wrap items-center justify-between gap-4">
		<div>
			<h1 class="text-2xl font-bold" style="color: var(--text-primary);">Tauschlisten</h1>
			<p class="mt-1 text-sm" style="color: var(--text-secondary);">
				Verwalte deine doppelten und gesuchten Hefte.
			</p>
		</div>
		<a
			href={resolve('/trades/wanted/add')}
			class="rounded-lg px-4 py-2 text-sm font-semibold"
			style="background: var(--color-brand-500); color: #000;"
			data-testid="add-wanted-link"
		>
			Wünsche hinzufügen
		</a>
	</header>

	<div class="mb-6 flex gap-2" role="tablist" aria-label="Tauschlisten">
		<button
			type="button"
			role="tab"
			id="offers-tab"
			aria-controls="offers-panel"
			aria-selected={activeTab === 'offers'}
			class="cursor-pointer rounded-lg px-4 py-2 text-sm font-semibold"
			style={activeTab === 'offers'
				? 'background: var(--color-status-duplicate); color: #000;'
				: 'background: var(--glass); color: var(--text-secondary);'}
			onclick={() => (activeTab = 'offers')}
			data-testid="offers-tab"
		>
			Tauschbar ({offersTotal})
		</button>
		<button
			type="button"
			role="tab"
			id="wanted-tab"
			aria-controls="wanted-panel"
			aria-selected={activeTab === 'wanted'}
			class="cursor-pointer rounded-lg px-4 py-2 text-sm font-semibold"
			style={activeTab === 'wanted'
				? 'background: var(--color-status-wanted); color: #000;'
				: 'background: var(--glass); color: var(--text-secondary);'}
			onclick={() => (activeTab = 'wanted')}
			data-testid="wanted-tab"
		>
			Wunschliste ({wantedTotal})
		</button>
	</div>

	{#if activeTab === 'offers' && offersError}
		<div class="glass-elevated mb-4 rounded-lg p-4" role="alert" data-testid="trades-error">
			<p style="color: var(--color-error);">{offersError}</p>
		</div>
	{:else if activeTab === 'wanted' && wantedError}
		<div class="glass-elevated mb-4 rounded-lg p-4" role="alert" data-testid="trades-error">
			<p style="color: var(--color-error);">{wantedError}</p>
		</div>
	{/if}

	<p class="sr-only" aria-live="polite">{announcement}</p>

	{#if activeTab === 'offers'}
		<div role="tabpanel" id="offers-panel" aria-labelledby="offers-tab">
			{#if loadingOffers}
				<p data-testid="offers-loading">Lade Tauschangebote …</p>
			{:else if offers.length === 0}
				<div class="glass-elevated rounded-lg p-8 text-center" data-testid="offers-empty">
					<h2 class="text-lg font-semibold">Noch keine Tauschangebote</h2>
					<p class="mt-2 text-sm" style="color: var(--text-secondary);">
						Markiere ein vorhandenes Heft als „Doppelt“, damit es hier erscheint.
					</p>
				</div>
			{:else}
				<div class="grid gap-4 sm:grid-cols-2 lg:grid-cols-3" data-testid="offers-list">
					{#each offers as offer (offer.entry_id)}
						{@const cover = coverSource(offer)}
						<article class="glass-elevated flex gap-4 rounded-lg p-4" data-testid="offer-card">
							{#if cover}
								<img
									src={cover}
									alt="Cover von {offer.series_name} #{offer.issue_number}: {offer.title}"
									class="h-24 w-16 flex-shrink-0 rounded object-cover"
								/>
							{:else}
								<div
									class="flex h-24 w-16 flex-shrink-0 items-center justify-center rounded"
									style="background: var(--glass-border);"
								>
									#{offer.issue_number}
								</div>
							{/if}
							<div class="min-w-0 flex-1">
								<p class="text-xs" style="color: var(--text-tertiary);">
									{offer.series_name} · #{offer.issue_number}
								</p>
								<h2 class="truncate font-semibold">{offer.title}</h2>
								<p class="mt-1 text-sm">Zustand {offer.condition_grade}</p>
								{#if offer.copy_number > 1}
									<p class="text-sm" style="color: var(--text-secondary);">
										Exemplar {offer.copy_number}
									</p>
								{/if}
								<div class="mt-3 flex flex-wrap gap-2">
									<a
										href={resolve(`/issues/${offer.issue_id}`)}
										class="text-sm underline"
										style="color: var(--color-brand-500);"
									>
										Details
									</a>
									<button
										type="button"
										class="cursor-pointer text-sm underline"
										style="color: var(--color-error);"
										onclick={() => deactivateOffer(offer)}
									>
										Nicht mehr tauschbar
									</button>
								</div>
							</div>
						</article>
					{/each}
				</div>
			{/if}

			{#if !loadingOffers && (offersPage > 1 || offersPage * PER_PAGE < offersTotal)}
				<nav class="mt-6 flex justify-center gap-3" aria-label="Angebotsseiten">
					<button
						type="button"
						disabled={offersPage <= 1}
						onclick={() => loadOffers(offersPage - 1)}
						class="rounded px-3 py-2 disabled:opacity-50"
					>
						Zurück
					</button>
					<button
						type="button"
						disabled={offersPage * PER_PAGE >= offersTotal}
						onclick={() => loadOffers(offersPage + 1)}
						class="rounded px-3 py-2 disabled:opacity-50"
					>
						Weiter
					</button>
				</nav>
			{/if}
		</div>
	{:else}
		<div role="tabpanel" id="wanted-panel" aria-labelledby="wanted-tab">
			{#if loadingWanted}
				<p data-testid="wanted-loading">Lade Wunschliste …</p>
			{:else if wanted.length === 0}
				<div class="glass-elevated rounded-lg p-8 text-center" data-testid="wanted-empty">
					<h2 class="text-lg font-semibold">Deine Wunschliste ist leer</h2>
					<p class="mt-2 text-sm" style="color: var(--text-secondary);">
						Wähle fehlende Hefte aus und füge sie gesammelt hinzu.
					</p>
				</div>
			{:else}
				<div class="grid gap-4 sm:grid-cols-2 lg:grid-cols-3" data-testid="wanted-list">
					{#each wanted as entry (entry.entry_id)}
						{@const cover = coverSource(entry)}
						<article class="glass-elevated flex gap-4 rounded-lg p-4" data-testid="wanted-card">
							{#if cover}
								<img
									src={cover}
									alt="Cover von {entry.series_name} #{entry.issue_number}: {entry.title}"
									class="h-24 w-16 flex-shrink-0 rounded object-cover"
								/>
							{:else}
								<div
									class="flex h-24 w-16 flex-shrink-0 items-center justify-center rounded"
									style="background: var(--glass-border);"
								>
									#{entry.issue_number}
								</div>
							{/if}
							<div class="min-w-0 flex-1">
								<p class="text-xs" style="color: var(--text-tertiary);">
									{entry.series_name} · #{entry.issue_number}
								</p>
								<h2 class="truncate font-semibold">{entry.title}</h2>
								{#if entry.copy_number > 1}
									<p class="mt-1 text-sm" style="color: var(--text-secondary);">
										Exemplar {entry.copy_number}
									</p>
								{/if}
								<div class="mt-3 flex flex-wrap gap-2">
									<a
										href={resolve(`/issues/${entry.issue_id}`)}
										class="text-sm underline"
										style="color: var(--color-brand-500);"
									>
										Als vorhanden markieren
									</a>
									<button
										type="button"
										class="cursor-pointer text-sm underline"
										style="color: var(--color-error);"
										onclick={() => removeWanted(entry)}
									>
										Entfernen
									</button>
								</div>
							</div>
						</article>
					{/each}
				</div>
			{/if}

			{#if !loadingWanted && (wantedPage > 1 || wantedPage * PER_PAGE < wantedTotal)}
				<nav class="mt-6 flex justify-center gap-3" aria-label="Wunschlistenseiten">
					<button
						type="button"
						disabled={wantedPage <= 1}
						onclick={() => loadWanted(wantedPage - 1)}
						class="rounded px-3 py-2 disabled:opacity-50"
					>
						Zurück
					</button>
					<button
						type="button"
						disabled={wantedPage * PER_PAGE >= wantedTotal}
						onclick={() => loadWanted(wantedPage + 1)}
						class="rounded px-3 py-2 disabled:opacity-50"
					>
						Weiter
					</button>
				</nav>
			{/if}
		</div>
	{/if}
</div>
