<script lang="ts">
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { getAuthState } from '$lib/stores/auth.svelte';
	import { fetchMatches, fetchOpenTrades, type Trade, type TradeMatch } from '$lib/api/trades';
	import TradeMatchCard from '$lib/components/trade/TradeMatchCard.svelte';
	import TradeSummaryCard from '$lib/components/trade/TradeSummaryCard.svelte';

	const auth = getAuthState();
	let activeTab = $state<'matches' | 'trades'>('matches');
	let matches = $state<TradeMatch[]>([]);
	let trades = $state<Trade[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);
	let loaded = false;

	$effect(() => {
		if (!auth.isLoading && !auth.isAuthenticated) {
			void goto(resolve('/login'));
		} else if (auth.isAuthenticated && !loaded) {
			loaded = true;
			void load();
		}
	});

	async function load() {
		loading = true;
		error = null;
		try {
			const [matchResult, tradeResult] = await Promise.all([
				fetchMatches({ per_page: 50 }),
				fetchOpenTrades({ per_page: 50 })
			]);
			matches = matchResult.data;
			trades = tradeResult.data;
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'Tauschdaten konnten nicht geladen werden.';
		} finally {
			loading = false;
		}
	}

	function proposed(trade: Trade) {
		trades = [trade, ...trades];
		matches = matches.map((match) =>
			match.id === trade.match_id
				? { ...match, open_trade_id: trade.id, open_trade_status: trade.status }
				: match
		);
		activeTab = 'trades';
	}
</script>

<svelte:head><title>Tausch – LILLY</title></svelte:head>

<div class="mx-auto min-h-[calc(100vh-3.5rem)] max-w-6xl px-4 py-8 sm:px-6 lg:px-8">
	<header class="mb-6 flex flex-wrap items-start justify-between gap-4">
		<div>
			<h1 class="text-2xl font-bold">Tausch</h1>
			<p class="mt-1 text-sm" style="color: var(--text-secondary);">
				Finde passende Sammler und koordiniere aktive Tausche.
			</p>
		</div>
		<nav class="flex flex-wrap gap-2" aria-label="Tauschlisten verwalten">
			<a
				href={resolve('/trades/offers')}
				class="rounded-lg px-3 py-2 text-sm"
				style="background: var(--glass);"
			>
				Tauschbare Hefte
			</a>
			<a
				href={resolve('/trades/wanted')}
				class="rounded-lg px-3 py-2 text-sm"
				style="background: var(--glass);"
			>
				Wunschliste
			</a>
		</nav>
	</header>

	<nav class="mb-6 flex gap-2" aria-label="Tauschbereiche">
		<button
			type="button"
			aria-pressed={activeTab === 'matches'}
			onclick={() => (activeTab = 'matches')}
			class="cursor-pointer rounded-lg px-4 py-2 text-sm font-semibold"
			style={activeTab === 'matches'
				? 'background: var(--color-brand-500); color: #000;'
				: 'background: var(--glass);'}
			data-testid="matches-tab"
		>
			Vorschläge ({matches.length})
		</button>
		<button
			type="button"
			aria-pressed={activeTab === 'trades'}
			onclick={() => (activeTab = 'trades')}
			class="cursor-pointer rounded-lg px-4 py-2 text-sm font-semibold"
			style={activeTab === 'trades'
				? 'background: var(--color-brand-500); color: #000;'
				: 'background: var(--glass);'}
			data-testid="active-trades-tab"
		>
			Aktive Tausche ({trades.length})
		</button>
	</nav>

	{#if error}
		<div class="glass-elevated rounded-lg p-4" role="alert">
			<p style="color: var(--color-error);">{error}</p>
		</div>
	{:else if loading}
		<p data-testid="trades-loading">Tauschdaten werden geladen …</p>
	{:else if activeTab === 'matches'}
		<div class="space-y-4" data-testid="matches-panel">
			{#if matches.length === 0}
				<div class="glass-elevated rounded-xl p-8 text-center">
					<h2 class="text-lg font-semibold">Noch keine Tauschvorschläge</h2>
					<p class="mt-2 text-sm" style="color: var(--text-secondary);">
						Sobald sich deine Tausch- und Wunschliste mit einer anderen Person gegenseitig ergänzen,
						erscheint hier ein Match.
					</p>
				</div>
			{:else}
				{#each matches as match (match.id)}
					<TradeMatchCard {match} onproposed={proposed} />
				{/each}
			{/if}
		</div>
	{:else}
		<div class="grid gap-4 sm:grid-cols-2" data-testid="active-trades-panel">
			{#if trades.length === 0}
				<div class="glass-elevated rounded-xl p-8 text-center sm:col-span-2">
					<h2 class="text-lg font-semibold">Noch keine offenen Tausche</h2>
					<p class="mt-2 text-sm" style="color: var(--text-secondary);">
						Erstelle aus einem Match deinen ersten Tauschvorschlag.
					</p>
				</div>
			{:else}
				{#each trades as trade (trade.id)}
					<TradeSummaryCard {trade} />
				{/each}
			{/if}
		</div>
	{/if}
</div>
