<script lang="ts">
	import { page } from '$app/state';
	import { resolve } from '$app/paths';
	import {
		fetchPublicCollectionStats,
		fetchPublicProfile,
		type PublicProfile
	} from '$lib/api/profile';
	import type { CollectionStats } from '$lib/api/collection';
	import SeriesProgressBar from '$lib/components/collection/SeriesProgressBar.svelte';

	let profile = $state<PublicProfile | null>(null);
	let stats = $state<CollectionStats | null>(null);
	let loading = $state(true);
	let notFound = $state(false);
	let error = $state<string | null>(null);
	let collectionPrivate = $state(false);
	let statsError = $state<string | null>(null);

	const userId = $derived(Number(page.params.id));

	$effect(() => {
		void userId;
		void loadProfile();
	});

	async function loadProfile() {
		loading = true;
		notFound = false;
		error = null;
		collectionPrivate = false;
		statsError = null;
		profile = null;
		stats = null;
		if (!Number.isSafeInteger(userId) || userId <= 0) {
			notFound = true;
			loading = false;
			return;
		}
		try {
			profile = await fetchPublicProfile(userId);
			try {
				stats = await fetchPublicCollectionStats(userId);
			} catch (cause) {
				if ((cause as Error & { status?: number }).status === 404) {
					collectionPrivate = true;
				} else {
					statsError =
						cause instanceof Error
							? cause.message
							: 'Sammlungsstatistik konnte nicht geladen werden.';
				}
			}
		} catch (cause) {
			if ((cause as Error & { status?: number }).status === 404) notFound = true;
			else error = cause instanceof Error ? cause.message : 'Profil konnte nicht geladen werden.';
		} finally {
			loading = false;
		}
	}
</script>

<svelte:head>
	<title>{profile ? `${profile.display_name} – LILLY` : 'Sammlerprofil – LILLY'}</title>
</svelte:head>

<div class="min-h-[calc(100vh-3.5rem)] px-4 py-8 sm:px-6 lg:px-8">
	<div class="mx-auto max-w-4xl">
		{#if loading}
			<p data-testid="public-profile-loading">Profil wird geladen …</p>
		{:else if notFound}
			<section class="glass-elevated rounded-lg p-8 text-center" data-testid="private-profile">
				<h1 class="text-2xl font-bold">Profil nicht gefunden</h1>
				<p class="mt-2 text-sm" style="color: var(--text-secondary);">
					Dieses Profil existiert nicht oder ist privat.
				</p>
			</section>
		{:else if error}
			<p role="alert" style="color: var(--color-error);">{error}</p>
		{:else if profile}
			<header class="glass-elevated mb-6 rounded-lg p-6" data-testid="public-profile">
				<h1 class="text-2xl font-bold">{profile.display_name}</h1>
				{#if profile.location}<p class="mt-1">{profile.location}</p>{/if}
				<p class="mt-1 text-sm" style="color: var(--text-tertiary);">
					Mitglied seit {new Date(profile.created_at).toLocaleDateString('de-DE')}
				</p>
			</header>

			{#if stats}
				<section class="glass-elevated rounded-lg p-6" aria-labelledby="public-stats-heading">
					<div class="mb-4 flex items-center justify-between gap-3">
						<h2 id="public-stats-heading" class="text-lg font-semibold">Sammlung</h2>
						<a href={resolve(`/users/${profile.id}/collection`)} class="text-sm underline">
							Sammlung öffnen
						</a>
					</div>
					{#each stats.series_stats as series (series.series_id)}
						<SeriesProgressBar
							series_name={series.series_name}
							owned_count={series.owned_count}
							total_count={series.total_in_series}
							progress_percent={series.progress_percent}
							duplicate_count={series.duplicate_count}
						/>
					{/each}
					{#if stats.series_stats.length === 0}
						<p class="text-sm" style="color: var(--text-tertiary);">
							Diese öffentliche Sammlung ist noch leer.
						</p>
					{/if}
				</section>
			{:else if collectionPrivate}
				<p class="text-sm" style="color: var(--text-tertiary);">Die Sammlung ist privat.</p>
			{:else if statsError}
				<p role="alert" style="color: var(--color-error);">{statsError}</p>
			{/if}
		{/if}
	</div>
</div>
