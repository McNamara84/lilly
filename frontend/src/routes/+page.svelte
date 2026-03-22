<script lang="ts">
	import { getAuthState, performLogout } from '$lib/stores/auth.svelte';
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { fetchCollectionStats, type CollectionStats } from '$lib/api/collection';
	import StatsCard from '$lib/components/stats/StatsCard.svelte';
	import SeriesProgressBar from '$lib/components/collection/SeriesProgressBar.svelte';

	const auth = getAuthState();

	let stats = $state<CollectionStats | null>(null);
	let statsLoading = $state(true);

	$effect(() => {
		if (!auth.isLoading && !auth.isAuthenticated) {
			goto(resolve('/login'));
		}
	});

	$effect(() => {
		if (auth.isAuthenticated) {
			loadStats();
		}
	});

	async function loadStats() {
		statsLoading = true;
		try {
			stats = await fetchCollectionStats();
		} catch {
			// Stats load failure is non-critical
		} finally {
			statsLoading = false;
		}
	}

	async function handleLogout() {
		await performLogout();
		await goto(resolve('/login'));
	}

	const hasCollection = $derived(stats !== null && stats.total_owned > 0);
</script>

<svelte:head>
	<title>Übersicht – LILLY</title>
</svelte:head>

<div class="min-h-[calc(100vh-3.5rem)] px-4 py-8 sm:px-6 lg:px-8">
	{#if auth.user}
		<!-- Welcome Header -->
		<div class="mb-8" data-testid="welcome-header">
			<h2 class="text-2xl font-bold" style="color: var(--text-primary);">
				Willkommen zurück, {auth.user.display_name}!
			</h2>
			<p class="text-sm mt-1" style="color: var(--text-secondary);">
				{new Date().toLocaleDateString('de-DE', {
					weekday: 'long',
					day: 'numeric',
					month: 'long',
					year: 'numeric'
				})}
			</p>
		</div>

		<!-- Stats Cards -->
		<div class="grid grid-cols-2 lg:grid-cols-4 gap-4 mb-8" data-testid="stats-row">
			<StatsCard label="Gesamte Hefte" value={stats?.total_owned ?? 0} icon="📚" />
			<StatsCard
				label="Sammlungsfortschritt"
				value="{(stats?.overall_progress_percent ?? 0).toFixed(1)}%"
				icon="📈"
			/>
			<StatsCard label="Tauschbar" value={stats?.total_duplicate ?? 0} icon="🔄" />
			<StatsCard label="Gesucht" value={stats?.total_wanted ?? 0} icon="🔍" />
		</div>

		<!-- Series progress bars -->
		{#if stats?.series_stats && stats.series_stats.length > 0}
			<section class="glass-elevated rounded-lg p-6 mb-8" data-testid="series-progress-section">
				<h3 class="text-lg font-semibold mb-4" style="color: var(--text-primary);">
					Serien-Fortschritt
				</h3>
				{#each stats.series_stats as s (s.series_id)}
					<SeriesProgressBar
						series_name={s.series_name}
						owned_count={s.owned_count}
						total_count={s.total_in_series}
						duplicate_count={s.duplicate_count}
					/>
				{/each}
			</section>
		{/if}

		<!-- Empty state or quick links -->
		{#if !hasCollection && !statsLoading}
			<div class="glass-elevated p-8 rounded-lg text-center" data-testid="empty-state">
				<p class="text-lg font-medium mb-2" style="color: var(--text-primary);">
					Deine Sammlung ist noch leer
				</p>
				<p class="text-sm" style="color: var(--text-secondary);">
					Beginne damit, Serien und Hefte zu deiner Sammlung hinzuzufügen.
				</p>
			</div>
		{/if}

		<!-- Quick links -->
		<div class="flex flex-wrap gap-3 mt-6 mb-8" data-testid="quick-links">
			<a
				href={resolve('/collection')}
				class="rounded-lg px-4 py-2.5 text-sm font-semibold"
				style="background: var(--color-brand-500); color: #000;"
			>
				Zur Sammlung
			</a>
			<a
				href={resolve('/collection/add')}
				class="rounded-lg px-4 py-2.5 text-sm font-semibold"
				style="background: var(--glass); border: 1px solid var(--glass-border); color: var(--text-primary);"
			>
				Hefte hinzufügen
			</a>
		</div>

		<!-- Trade suggestions placeholder -->
		<section class="glass-elevated rounded-lg p-6 mb-6" data-testid="trade-placeholder">
			<h3 class="text-lg font-semibold mb-2" style="color: var(--text-primary);">
				Tausch-Vorschläge
			</h3>
			<p class="text-sm" style="color: var(--text-tertiary);">
				Noch keine Tausch-Vorschläge verfügbar.
			</p>
		</section>

		<!-- Activity timeline placeholder -->
		<section class="glass-elevated rounded-lg p-6 mb-6" data-testid="activity-placeholder">
			<h3 class="text-lg font-semibold mb-2" style="color: var(--text-primary);">
				Letzte Aktivitäten
			</h3>
			<p class="text-sm" style="color: var(--text-tertiary);">Noch keine Aktivitäten vorhanden.</p>
		</section>

		<!-- Logout -->
		<div class="mt-8 text-center">
			<button
				onclick={handleLogout}
				class="rounded-lg px-6 py-2.5 text-sm font-medium transition-colors cursor-pointer"
				style="background-color: var(--surface-raised); border: 1px solid var(--glass-border); color: var(--text-secondary);"
				data-testid="logout-button"
			>
				Abmelden
			</button>
		</div>
	{/if}
</div>
