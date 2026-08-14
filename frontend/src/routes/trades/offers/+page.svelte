<script lang="ts">
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { getAuthState } from '$lib/stores/auth.svelte';
	import { updateCollectionEntry } from '$lib/api/collection';
	import { fetchTradeOffers, type TradeOffer } from '$lib/api/trades';

	const auth = getAuthState();
	let offers = $state<TradeOffer[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);
	let loaded = false;

	$effect(() => {
		if (!auth.isLoading && !auth.isAuthenticated) void goto(resolve('/login'));
		else if (auth.isAuthenticated && !loaded) {
			loaded = true;
			void load();
		}
	});

	async function load() {
		try {
			offers = (await fetchTradeOffers({ per_page: 100 })).data;
		} catch (cause) {
			error =
				cause instanceof Error ? cause.message : 'Tauschangebote konnten nicht geladen werden.';
		} finally {
			loading = false;
		}
	}

	async function deactivate(offer: TradeOffer) {
		try {
			await updateCollectionEntry(offer.entry_id, { status: 'owned' });
			offers = offers.filter((item) => item.entry_id !== offer.entry_id);
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'Angebot konnte nicht entfernt werden.';
		}
	}
</script>

<svelte:head><title>Tauschbare Hefte – LILLY</title></svelte:head>
<div class="mx-auto max-w-5xl px-4 py-8 sm:px-6">
	<header class="mb-6">
		<a href={resolve('/trades')} class="text-sm underline">← Zurück zum Tausch</a>
		<h1 class="mt-3 text-2xl font-bold">Tauschbare Hefte</h1>
	</header>
	{#if error}<p role="alert" style="color: var(--color-error);">{error}</p>{/if}
	{#if loading}
		<p>Wird geladen …</p>
	{:else if offers.length === 0}
		<p class="glass-elevated rounded-xl p-8 text-center">Noch keine Tauschangebote.</p>
	{:else}
		<div class="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
			{#each offers as offer (offer.entry_id)}
				<article class="glass-elevated rounded-xl p-4" data-testid="offer-card">
					<p class="text-xs" style="color: var(--text-secondary);">
						{offer.series_name} #{offer.issue_number}
					</p>
					<h2 class="font-semibold">{offer.title}</h2>
					<p class="mt-1 text-sm">Zustand {offer.condition_grade}</p>
					{#if offer.edition_label}
						<p class="mt-1 text-xs" style="color: var(--text-secondary);">
							{offer.edition_label} · Exemplar {offer.copy_number}
						</p>
					{/if}
					<button
						type="button"
						onclick={() => deactivate(offer)}
						class="mt-3 cursor-pointer text-sm underline"
						style="color: var(--color-error);"
					>
						Nicht mehr tauschbar
					</button>
				</article>
			{/each}
		</div>
	{/if}
</div>
