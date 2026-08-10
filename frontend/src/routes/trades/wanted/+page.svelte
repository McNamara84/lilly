<script lang="ts">
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { getAuthState } from '$lib/stores/auth.svelte';
	import { deleteWantedEntry, fetchWantedEntries, type WantedEntry } from '$lib/api/trades';

	const auth = getAuthState();
	let entries = $state<WantedEntry[]>([]);
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
			entries = (await fetchWantedEntries({ per_page: 100 })).data;
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'Wunschliste konnte nicht geladen werden.';
		} finally {
			loading = false;
		}
	}

	async function remove(entry: WantedEntry) {
		try {
			await deleteWantedEntry(entry.entry_id);
			entries = entries.filter((item) => item.entry_id !== entry.entry_id);
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'Wunsch konnte nicht entfernt werden.';
		}
	}
</script>

<svelte:head><title>Wunschliste – LILLY</title></svelte:head>
<div class="mx-auto max-w-5xl px-4 py-8 sm:px-6">
	<header class="mb-6 flex flex-wrap items-end justify-between gap-3">
		<div>
			<a href={resolve('/trades')} class="text-sm underline">← Zurück zum Tausch</a>
			<h1 class="mt-3 text-2xl font-bold">Wunschliste</h1>
		</div>
		<a
			href={resolve('/trades/wanted/add')}
			class="rounded-lg px-4 py-2 text-sm font-semibold"
			style="background: var(--color-brand-500); color: #000;"
		>
			Wünsche hinzufügen
		</a>
	</header>
	{#if error}<p role="alert" style="color: var(--color-error);">{error}</p>{/if}
	{#if loading}
		<p>Wird geladen …</p>
	{:else if entries.length === 0}
		<p class="glass-elevated rounded-xl p-8 text-center">Deine Wunschliste ist leer.</p>
	{:else}
		<div class="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
			{#each entries as entry (entry.entry_id)}
				<article class="glass-elevated rounded-xl p-4" data-testid="wanted-card">
					<p class="text-xs" style="color: var(--text-secondary);">
						{entry.series_name} #{entry.issue_number}
					</p>
					<h2 class="font-semibold">{entry.title}</h2>
					<button
						type="button"
						onclick={() => remove(entry)}
						class="mt-3 cursor-pointer text-sm underline"
						style="color: var(--color-error);"
					>
						Entfernen
					</button>
				</article>
			{/each}
		</div>
	{/if}
</div>
