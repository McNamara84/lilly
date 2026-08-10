<script lang="ts">
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { getAuthState } from '$lib/stores/auth.svelte';
	import { fetchMessageThreads, type MessageThreadSummary } from '$lib/api/messages';

	const auth = getAuthState();
	let threads = $state<MessageThreadSummary[]>([]);
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
			threads = (await fetchMessageThreads({ per_page: 100 })).data;
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'Nachrichten konnten nicht geladen werden.';
		} finally {
			loading = false;
		}
	}
</script>

<svelte:head><title>Nachrichten – LILLY</title></svelte:head>
<div class="mx-auto max-w-4xl px-4 py-8 sm:px-6">
	<h1 class="text-2xl font-bold">Nachrichten</h1>
	<p class="mt-1 text-sm" style="color: var(--text-secondary);">
		Unterhaltungen zu deinen Tauschen.
	</p>
	{#if error}<p class="mt-5" role="alert" style="color: var(--color-error);">{error}</p>{/if}
	{#if loading}
		<p class="mt-6">Nachrichten werden geladen …</p>
	{:else if threads.length === 0}
		<div class="glass-elevated mt-6 rounded-xl p-8 text-center">Noch keine Unterhaltungen.</div>
	{:else}
		<div class="mt-6 space-y-3" data-testid="message-thread-list">
			{#each threads as thread (thread.id)}
				<a
					href={resolve(`/messages/${thread.id}`)}
					class="glass-elevated flex items-center gap-4 rounded-xl p-4"
				>
					<div class="min-w-0 flex-1">
						<div class="flex items-center gap-2">
							<h2 class="truncate font-semibold">{thread.partner.display_name}</h2>
							{#if thread.unread_count > 0}
								<span
									class="rounded-full px-2 py-0.5 text-xs font-bold"
									style="background: var(--color-brand-500); color: #000;"
								>
									{thread.unread_count}
								</span>
							{/if}
						</div>
						<p class="truncate text-sm" style="color: var(--text-secondary);">
							{thread.last_message ?? 'Noch keine Nachricht'}
						</p>
					</div>
					<span class="text-xs" style="color: var(--text-tertiary);">
						{new Date(thread.last_message_at ?? thread.updated_at).toLocaleDateString('de-DE')}
					</span>
				</a>
			{/each}
		</div>
	{/if}
</div>
