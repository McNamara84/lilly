<script lang="ts">
	import { page } from '$app/state';
	import { fetchPublicCollection, type PublicCollectionEntry } from '$lib/api/profile';
	import CollectionNote from '$lib/components/collection/CollectionNote.svelte';

	let entries = $state<PublicCollectionEntry[]>([]);
	let loading = $state(true);
	let notFound = $state(false);
	let error = $state<string | null>(null);

	const userId = $derived(Number(page.params.id));

	$effect(() => {
		void loadCollection(userId);
	});

	async function loadCollection(targetUserId: number) {
		entries = [];
		loading = true;
		notFound = false;
		error = null;
		if (!Number.isSafeInteger(targetUserId) || targetUserId <= 0) {
			notFound = true;
			loading = false;
			return;
		}

		try {
			let currentPage = 1;
			let total = Number.POSITIVE_INFINITY;
			const loadedEntries: PublicCollectionEntry[] = [];
			while (loadedEntries.length < total) {
				const result = await fetchPublicCollection(targetUserId, currentPage, 100);
				loadedEntries.push(...result.data);
				total = result.total;
				if (result.data.length === 0) break;
				currentPage += 1;
			}
			entries = loadedEntries;
		} catch (cause) {
			if ((cause as Error & { status?: number }).status === 404) notFound = true;
			else error = cause instanceof Error ? cause.message : 'Sammlung konnte nicht geladen werden.';
		} finally {
			loading = false;
		}
	}

	function statusLabel(status: PublicCollectionEntry['status']): string {
		if (status === 'owned') return 'Vorhanden';
		if (status === 'duplicate') return 'Doppelt/Tauschbar';
		return 'Gesucht';
	}
</script>

<svelte:head>
	<title>Öffentliche Sammlung – LILLY</title>
</svelte:head>

<div class="min-h-[calc(100vh-3.5rem)] px-4 py-8 sm:px-6 lg:px-8">
	<div class="mx-auto max-w-6xl">
		<h1 class="mb-6 text-2xl font-bold">Öffentliche Sammlung</h1>

		{#if loading}
			<p data-testid="public-collection-loading">Sammlung wird geladen …</p>
		{:else if notFound}
			<section class="glass-elevated rounded-lg p-8 text-center" data-testid="private-collection">
				<h2 class="text-xl font-semibold">Sammlung nicht gefunden</h2>
				<p class="mt-2 text-sm" style="color: var(--text-secondary);">
					Diese Sammlung existiert nicht oder ist privat.
				</p>
			</section>
		{:else if error}
			<p role="alert" style="color: var(--color-error);">{error}</p>
		{:else if entries.length === 0}
			<p data-testid="public-collection-empty">Diese öffentliche Sammlung ist noch leer.</p>
		{:else}
			<div
				class="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3"
				data-testid="public-collection-grid"
			>
				{#each entries as entry (`${entry.issue_id}-${entry.copy_number}`)}
					<article class="glass-elevated rounded-lg p-4" data-testid="public-collection-entry">
						<div class="flex gap-4">
							{#if entry.cover_local_path || entry.cover_url}
								<img
									src={entry.cover_local_path ?? entry.cover_url}
									alt="Cover von {entry.series_name} #{entry.issue_number}: {entry.title}"
									class="h-36 w-24 rounded object-cover"
								/>
							{:else}
								<div
									class="flex h-36 w-24 items-center justify-center rounded"
									style="background: var(--glass);"
								>
									#{entry.issue_number}
								</div>
							{/if}
							<div class="min-w-0">
								<p class="text-xs" style="color: var(--text-tertiary);">
									{entry.series_name} #{entry.issue_number}
								</p>
								<h2 class="font-semibold">{entry.title}</h2>
								<p class="mt-2 text-xs">{statusLabel(entry.status)} · {entry.condition_grade}</p>
							</div>
						</div>
						<div class="mt-4 border-t pt-3" style="border-color: var(--glass-border);">
							<h3 class="mb-1 text-xs font-semibold uppercase tracking-wide">Persönliche Notiz</h3>
							<CollectionNote note={entry.notes} />
						</div>
					</article>
				{/each}
			</div>
		{/if}
	</div>
</div>
