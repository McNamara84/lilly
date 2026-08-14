<script lang="ts">
	import { page } from '$app/stores';
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { getAuthState } from '$lib/stores/auth.svelte';
	import {
		acceptTrade,
		cancelTrade,
		completeTrade,
		fetchTrade,
		type Trade,
		type TradeItem
	} from '$lib/api/trades';
	import MessageThread from '$lib/components/messages/MessageThread.svelte';

	const auth = getAuthState();
	const tradeId = $derived(Number($page.params.id));
	let trade = $state<Trade | null>(null);
	let loading = $state(true);
	let actionPending = $state(false);
	let error = $state<string | null>(null);

	$effect(() => {
		if (!auth.isLoading && !auth.isAuthenticated) void goto(resolve('/login'));
		else if (auth.isAuthenticated) void load();
	});

	async function load() {
		if (!Number.isInteger(tradeId) || tradeId < 1) {
			error = 'Ungültige Tausch-ID.';
			loading = false;
			return;
		}
		try {
			trade = await fetchTrade(tradeId);
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'Tausch konnte nicht geladen werden.';
		} finally {
			loading = false;
		}
	}

	async function accept() {
		actionPending = true;
		try {
			trade = await acceptTrade(tradeId);
			error = null;
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'Tausch konnte nicht angenommen werden.';
		} finally {
			actionPending = false;
		}
	}

	async function cancel() {
		actionPending = true;
		try {
			await cancelTrade(tradeId);
			if (trade)
				trade = { ...trade, status: 'cancelled', cancellation_reason: 'cancelled_by_participant' };
			error = null;
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'Tausch konnte nicht abgebrochen werden.';
		} finally {
			actionPending = false;
		}
	}

	async function complete() {
		actionPending = true;
		try {
			trade = await completeTrade(tradeId);
			error = null;
		} catch (cause) {
			error =
				cause instanceof Error ? cause.message : 'Tauschabschluss konnte nicht bestätigt werden.';
		} finally {
			actionPending = false;
		}
	}

	function statusLabel(status: Trade['status']): string {
		return {
			proposed: 'Vorgeschlagen',
			accepted: 'Aktiv',
			cancelled: 'Abgebrochen',
			completed: 'Abgeschlossen'
		}[status];
	}

	function itemLabel(item: TradeItem): string {
		return `${item.series_name} #${item.issue_number}: ${item.title} · ${item.condition_grade}${item.edition_label ? ` · ${item.edition_label}` : ''}`;
	}
</script>

<svelte:head><title>Tausch-Details – LILLY</title></svelte:head>
<div class="mx-auto max-w-5xl px-4 py-8 sm:px-6">
	<a href={resolve('/trades')} class="text-sm underline">← Zurück zum Tausch</a>
	{#if loading}
		<p class="mt-6">Tausch wird geladen …</p>
	{:else if error && !trade}
		<p class="mt-6" role="alert" style="color: var(--color-error);">{error}</p>
	{:else if trade}
		<header class="mt-5 flex flex-wrap items-start justify-between gap-4">
			<div>
				<h1 class="text-2xl font-bold">Tausch mit {trade.partner.display_name}</h1>
				<p class="mt-1 text-sm" style="color: var(--text-secondary);">
					Vorgeschlagen am {new Date(trade.proposed_at).toLocaleString('de-DE')}
				</p>
			</div>
			<span class="rounded-full px-4 py-2 text-sm font-semibold" style="background: var(--glass);">
				{statusLabel(trade.status)}
			</span>
		</header>

		<div class="mt-6 grid gap-5 sm:grid-cols-2">
			<section class="glass-elevated rounded-xl p-5">
				<h2 class="mb-3 font-semibold">Du bietest</h2>
				<ul class="space-y-2 text-sm">
					{#each trade.my_offers as item (item.issue_id + ':' + item.copy_number)}
						<li>{itemLabel(item)}</li>
					{/each}
				</ul>
			</section>
			<section class="glass-elevated rounded-xl p-5">
				<h2 class="mb-3 font-semibold">Du erhältst</h2>
				<ul class="space-y-2 text-sm">
					{#each trade.partner_offers as item (item.issue_id + ':' + item.copy_number)}
						<li>{itemLabel(item)}</li>
					{/each}
				</ul>
			</section>
		</div>

		{#if trade.status === 'accepted' || trade.status === 'completed'}
			<section
				class="glass-elevated mt-5 rounded-xl p-5"
				aria-labelledby="completion-heading"
				aria-live="polite"
				data-testid="trade-completion"
			>
				<h2 id="completion-heading" class="font-semibold">Tauschabschluss</h2>
				<p class="mt-2 text-sm" style="color: var(--text-secondary);">
					Die Sammlungen werden erst aktualisiert, wenn beide Seiten den Erhalt bestätigt haben.
				</p>
				<ul class="mt-3 space-y-1 text-sm">
					<li>
						Deine Bestätigung:
						{trade.my_completion_confirmed_at
							? new Date(trade.my_completion_confirmed_at).toLocaleString('de-DE')
							: 'ausstehend'}
					</li>
					<li>
						Bestätigung von {trade.partner.display_name}:
						{trade.partner_completion_confirmed_at
							? new Date(trade.partner_completion_confirmed_at).toLocaleString('de-DE')
							: 'ausstehend'}
					</li>
				</ul>
				{#if trade.status === 'accepted' && !trade.my_completion_confirmed_at}
					<button
						type="button"
						onclick={complete}
						disabled={actionPending}
						class="mt-4 cursor-pointer rounded-lg px-4 py-2 font-semibold disabled:opacity-50"
						style="background: var(--color-brand-500); color: #000;"
						data-testid="complete-trade-button"
					>
						Tausch als erhalten bestätigen
					</button>
				{:else if trade.status === 'accepted'}
					<p class="mt-3 text-sm" data-testid="completion-waiting">
						Warten auf die Bestätigung der anderen Seite.
					</p>
				{:else if trade.completed_at}
					<p class="mt-3 text-sm" data-testid="completion-finished">
						Abgeschlossen am {new Date(trade.completed_at).toLocaleString('de-DE')}.
					</p>
				{/if}
			</section>
		{/if}

		{#if error}<p class="mt-4" role="alert" style="color: var(--color-error);">{error}</p>{/if}
		{#if trade.status === 'proposed' || trade.status === 'accepted'}
			<div class="mt-5 flex flex-wrap gap-3">
				{#if trade.status === 'proposed' && trade.role === 'responder'}
					<button
						type="button"
						onclick={accept}
						disabled={actionPending}
						class="cursor-pointer rounded-lg px-4 py-2 font-semibold disabled:opacity-50"
						style="background: var(--color-brand-500); color: #000;"
					>
						Tausch annehmen
					</button>
				{/if}
				<button
					type="button"
					onclick={cancel}
					disabled={actionPending}
					class="cursor-pointer rounded-lg px-4 py-2 font-semibold disabled:opacity-50"
					style="background: var(--glass); color: var(--color-error);"
				>
					Tausch abbrechen
				</button>
			</div>
		{/if}

		<section class="mt-8">
			<h2 class="mb-3 text-xl font-semibold">Nachrichten</h2>
			<MessageThread threadId={trade.thread_id} />
		</section>
	{/if}
</div>
