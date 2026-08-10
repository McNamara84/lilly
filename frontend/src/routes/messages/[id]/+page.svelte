<script lang="ts">
	import { page } from '$app/stores';
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { getAuthState } from '$lib/stores/auth.svelte';
	import MessageThread from '$lib/components/messages/MessageThread.svelte';

	const auth = getAuthState();
	const threadId = $derived(Number($page.params.id));
	$effect(() => {
		if (!auth.isLoading && !auth.isAuthenticated) void goto(resolve('/login'));
	});
</script>

<svelte:head><title>Unterhaltung – LILLY</title></svelte:head>
<div class="mx-auto max-w-4xl px-4 py-8 sm:px-6">
	<a href={resolve('/messages')} class="text-sm underline">← Zurück zu Nachrichten</a>
	<h1 class="mb-5 mt-3 text-2xl font-bold">Unterhaltung</h1>
	{#if auth.isLoading}
		<p data-testid="message-auth-loading">Anmeldung wird geprüft …</p>
	{:else if auth.isAuthenticated}
		{#if Number.isInteger(threadId) && threadId > 0}
			<MessageThread {threadId} />
		{:else}
			<p role="alert" style="color: var(--color-error);">Ungültige Thread-ID.</p>
		{/if}
	{/if}
</div>
