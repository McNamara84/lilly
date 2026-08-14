<script lang="ts">
	import { resolve } from '$app/paths';
	import type { Trade } from '$lib/api/trades';

	let { trade }: { trade: Trade } = $props();

	const label = $derived(
		trade.status === 'completed'
			? 'Abgeschlossen'
			: trade.status === 'cancelled'
				? 'Abgebrochen'
				: trade.status === 'accepted'
					? 'Aktiv'
					: trade.role === 'responder'
						? 'Vorschlag erhalten'
						: 'Vorgeschlagen'
	);
</script>

<article class="glass-elevated rounded-xl p-5" data-testid="trade-summary-card">
	<header class="flex items-start justify-between gap-3">
		<div>
			<h2 class="font-semibold">Tausch mit {trade.partner.display_name}</h2>
			<p class="mt-1 text-sm" style="color: var(--text-secondary);">
				{trade.my_offers.length} für {trade.partner_offers.length} Hefte
			</p>
		</div>
		<span class="rounded-full px-3 py-1 text-xs font-semibold" style="background: var(--glass);">
			{label}
		</span>
	</header>
	<a
		href={resolve(`/trades/${trade.id}`)}
		class="mt-4 inline-block text-sm font-semibold underline"
		style="color: var(--color-brand-500);"
	>
		Details und Nachrichten
	</a>
</article>
