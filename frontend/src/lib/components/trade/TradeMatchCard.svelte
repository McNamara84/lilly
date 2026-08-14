<script lang="ts">
	import { resolve } from '$app/paths';
	import {
		createTradeProposal,
		type MatchIssue,
		type Trade,
		type TradeMatch
	} from '$lib/api/trades';

	let {
		match,
		onproposed
	}: {
		match: TradeMatch;
		onproposed?: (trade: Trade) => void;
	} = $props();

	let offered = $state<number[]>([]);
	let requested = $state<number[]>([]);
	let submitting = $state(false);
	let error = $state<string | null>(null);

	$effect(() => {
		offered = match.my_offers.map((item) => item.entry_id);
		requested = match.partner_offers.map((item) => item.entry_id);
	});

	function toggle(values: number[], entryId: number): number[] {
		return values.includes(entryId) ? values.filter((id) => id !== entryId) : [...values, entryId];
	}

	function cover(item: MatchIssue): string | null {
		return item.cover_local_path ?? item.cover_url;
	}

	async function propose() {
		if (offered.length === 0 || requested.length === 0 || submitting) return;
		submitting = true;
		error = null;
		try {
			const trade = await createTradeProposal(match.id, offered, requested);
			onproposed?.(trade);
		} catch (cause) {
			error =
				cause instanceof Error ? cause.message : 'Tauschvorschlag konnte nicht erstellt werden.';
		} finally {
			submitting = false;
		}
	}
</script>

<article class="glass-elevated rounded-xl p-5" data-testid="trade-match-card">
	<header class="mb-4 flex items-center justify-between gap-3">
		<div class="flex min-w-0 items-center gap-3">
			{#if match.partner.avatar_path}
				<img src={match.partner.avatar_path} alt="" class="h-10 w-10 rounded-full object-cover" />
			{:else}
				<div
					class="flex h-10 w-10 items-center justify-center rounded-full font-bold"
					style="background: var(--glass);"
					aria-hidden="true"
				>
					{match.partner.display_name.slice(0, 1).toUpperCase()}
				</div>
			{/if}
			<div class="min-w-0">
				<h2 class="truncate font-semibold">{match.partner.display_name}</h2>
				{#if match.partner.location}
					<p class="truncate text-xs" style="color: var(--text-secondary);">
						{match.partner.location}
					</p>
				{/if}
			</div>
		</div>
		<div
			class="flex h-12 w-12 flex-shrink-0 items-center justify-center rounded-full text-sm font-bold"
			style="border: 3px solid var(--color-brand-500);"
			aria-label="Match-Score {match.match_score} Prozent"
		>
			{match.match_score}%
		</div>
	</header>

	<div class="grid gap-5 sm:grid-cols-2">
		<section aria-label="Du bietest im Tausch mit {match.partner.display_name}">
			<h3 class="mb-2 text-sm font-semibold">Du bietest</h3>
			<div class="space-y-2">
				{#each match.my_offers as item (item.entry_id)}
					<label
						class="flex cursor-pointer items-center gap-3 rounded-lg p-2"
						style="background: var(--glass);"
					>
						<input
							type="checkbox"
							checked={offered.includes(item.entry_id)}
							onchange={() => (offered = toggle(offered, item.entry_id))}
						/>
						{#if cover(item)}
							<img src={cover(item) ?? ''} alt="" class="h-12 w-8 rounded object-cover" />
						{/if}
						<span class="min-w-0 text-sm">
							<span class="block truncate">{item.series_name} #{item.issue_number}</span>
							<span class="block text-xs" style="color: var(--text-secondary);">
								{item.title} · {item.condition_grade}{item.edition_label
									? ` · ${item.edition_label}`
									: ''}
							</span>
						</span>
					</label>
				{/each}
			</div>
		</section>

		<section aria-label="Du erhältst im Tausch mit {match.partner.display_name}">
			<h3 class="mb-2 text-sm font-semibold">Du erhältst</h3>
			<div class="space-y-2">
				{#each match.partner_offers as item (item.entry_id)}
					<label
						class="flex cursor-pointer items-center gap-3 rounded-lg p-2"
						style="background: var(--glass);"
					>
						<input
							type="checkbox"
							checked={requested.includes(item.entry_id)}
							onchange={() => (requested = toggle(requested, item.entry_id))}
						/>
						{#if cover(item)}
							<img src={cover(item) ?? ''} alt="" class="h-12 w-8 rounded object-cover" />
						{/if}
						<span class="min-w-0 text-sm">
							<span class="block truncate">{item.series_name} #{item.issue_number}</span>
							<span class="block text-xs" style="color: var(--text-secondary);">
								{item.title} · {item.condition_grade}{item.edition_label
									? ` · ${item.edition_label}`
									: ''}
							</span>
						</span>
					</label>
				{/each}
			</div>
		</section>
	</div>

	{#if error}
		<p class="mt-3 text-sm" role="alert" style="color: var(--color-error);">{error}</p>
	{/if}

	<footer class="mt-4 flex justify-end">
		{#if match.open_trade_id}
			<a
				href={resolve(`/trades/${match.open_trade_id}`)}
				class="rounded-lg px-4 py-2 text-sm font-semibold"
				style="background: var(--glass);"
			>
				Offenen Tausch ansehen
			</a>
		{:else}
			<button
				type="button"
				disabled={submitting || offered.length === 0 || requested.length === 0}
				onclick={propose}
				class="cursor-pointer rounded-lg px-4 py-2 text-sm font-semibold disabled:cursor-not-allowed disabled:opacity-50"
				style="background: var(--color-brand-500); color: #000;"
			>
				{submitting ? 'Wird vorgeschlagen …' : 'Tausch vorschlagen'}
			</button>
		{/if}
	</footer>
</article>
