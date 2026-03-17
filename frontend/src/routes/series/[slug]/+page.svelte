<script lang="ts">
	import { untrack } from 'svelte';
	import { page } from '$app/stores';
	import { fetchSeriesIssues, fetchSeries, type Series, type Issue } from '$lib/api/series';

	let series = $state<Series | null>(null);
	let issues = $state<Issue[]>([]);
	let total = $state(0);
	let currentPage = $state(1);
	let loading = $state(true);
	let error = $state<string | null>(null);

	const slug = $derived($page.params.slug ?? '');

	async function loadData() {
		if (!slug) {
			error = 'Serie nicht gefunden';
			loading = false;
			return;
		}
		loading = true;
		error = null;
		try {
			const allSeries = await fetchSeries();
			series = allSeries.find((s) => s.slug === slug) ?? null;

			if (!series) {
				error = 'Serie nicht gefunden';
				loading = false;
				return;
			}

			const result = await fetchSeriesIssues(slug, 1);
			issues = result.data;
			total = result.total;
			currentPage = 1;
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load series';
		} finally {
			loading = false;
		}
	}

	async function goToPage(p: number) {
		try {
			currentPage = p;
			const result = await fetchSeriesIssues(slug, p);
			issues = result.data;
			total = result.total;
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load issues';
		}
	}

	const totalPages = $derived(Math.ceil(total / 50));

	$effect(() => {
		// Only re-run when slug changes; pagination is handled by goToPage()
		void slug;
		untrack(() => {
			loadData();
		});
	});
</script>

<svelte:head>
	<title>{series?.name ?? 'Serie'} – LILLY</title>
</svelte:head>

<div class="min-h-[calc(100vh-3.5rem)] px-4 py-8 sm:px-6 lg:px-8">
	{#if loading}
		<p style="color: var(--text-secondary);" data-testid="loading-indicator">Lade Serie...</p>
	{:else if error}
		<div
			class="p-4 rounded-lg mb-4"
			style="background-color: var(--color-error-100); color: var(--color-error-700);"
			role="alert"
			data-testid="error-message"
		>
			{error}
		</div>
	{:else if series}
		<!-- Series Header -->
		<header class="mb-8" data-testid="series-header">
			<h1 class="text-2xl font-bold mb-2" style="color: var(--text-primary);">
				{series.name}
			</h1>
			<div class="flex flex-wrap gap-4 text-sm" style="color: var(--text-secondary);">
				{#if series.publisher}
					<span>Verlag: {series.publisher}</span>
				{/if}
				{#if series.genre}
					<span>Genre: {series.genre}</span>
				{/if}
				{#if series.frequency}
					<span>Erscheinungsweise: {series.frequency}</span>
				{/if}
				{#if series.total_issues}
					<span>{series.total_issues} Hefte</span>
				{/if}
			</div>
		</header>

		<!-- Issue Grid -->
		{#if issues.length === 0}
			<p style="color: var(--text-secondary);" data-testid="empty-issues">
				Noch keine Hefte verfügbar.
			</p>
		{:else}
			<div
				class="grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-6 gap-4"
				data-testid="issues-grid"
			>
				{#each issues as issue (issue.id)}
					<div class="glass-elevated rounded-lg p-3 text-center" data-testid="issue-card">
						{#if issue.cover_local_path}
							<div
								class="aspect-[3/4] mb-2 rounded overflow-hidden"
								style="background-color: var(--surface-base);"
							>
								<img
									src={issue.cover_local_path}
									alt="Cover von {issue.title}"
									class="w-full h-full object-cover"
									loading="lazy"
								/>
							</div>
						{:else}
							<div
								class="aspect-[3/4] mb-2 rounded flex items-center justify-center"
								style="background-color: var(--surface-base);"
								role="img"
								aria-label="Kein Cover verfügbar"
							>
								<span class="text-2xl font-bold" style="color: var(--text-tertiary);">
									{issue.issue_number}
								</span>
							</div>
						{/if}
						<p class="text-xs font-medium truncate" style="color: var(--text-primary);">
							Nr. {issue.issue_number}
						</p>
						<p class="text-xs truncate" style="color: var(--text-secondary);">
							{issue.title}
						</p>
					</div>
				{/each}
			</div>

			<!-- Pagination -->
			{#if totalPages > 1}
				<nav
					class="flex justify-center gap-2 mt-8"
					aria-label="Seiten-Navigation"
					data-testid="pagination"
				>
					{#each Array.from({ length: totalPages }, (_, i) => i + 1) as p (p)}
						<button
							onclick={() => goToPage(p)}
							class="px-3 py-1.5 rounded text-sm cursor-pointer transition-colors"
							style={p === currentPage
								? 'background-color: var(--color-brand-500); color: white;'
								: 'background-color: var(--surface-raised); color: var(--text-secondary);'}
							aria-current={p === currentPage ? 'page' : undefined}
						>
							{p}
						</button>
					{/each}
				</nav>
			{/if}
		{/if}
	{/if}
</div>
