<script lang="ts">
	import { fetchMessages, markThreadRead, sendMessage, type TradeMessage } from '$lib/api/messages';

	let { threadId }: { threadId: number } = $props();

	let messages = $state<TradeMessage[]>([]);
	let content = $state('');
	let loading = $state(true);
	let sending = $state(false);
	let error = $state<string | null>(null);
	let announcement = $state('');

	async function load(silent = false) {
		if (!silent) loading = true;
		try {
			const result = await fetchMessages(threadId, { limit: 100 });
			messages = result.data;
			const lastIncoming = [...messages].reverse().find((message) => !message.is_mine);
			if (lastIncoming) await markThreadRead(threadId, lastIncoming.id);
			error = null;
		} catch (cause) {
			if (!silent) {
				error =
					cause instanceof Error ? cause.message : 'Nachrichten konnten nicht geladen werden.';
			}
		} finally {
			if (!silent) loading = false;
		}
	}

	$effect(() => {
		void load();
		const interval = window.setInterval(() => {
			if (!document.hidden) void load(true);
		}, 10_000);
		return () => window.clearInterval(interval);
	});

	async function submit() {
		const trimmed = content.trim();
		if (!trimmed || trimmed.length > 4000 || sending) return;
		sending = true;
		error = null;
		try {
			const message = await sendMessage(threadId, trimmed);
			if (!messages.some((candidate) => candidate.id === message.id))
				messages = [...messages, message];
			content = '';
			announcement = 'Nachricht gesendet.';
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'Nachricht konnte nicht gesendet werden.';
		} finally {
			sending = false;
		}
	}
</script>

<section
	class="glass-elevated rounded-xl"
	data-testid="message-thread"
	aria-label="Nachrichtenverlauf"
>
	<div class="max-h-[28rem] space-y-3 overflow-y-auto p-4" aria-live="polite">
		{#if loading}
			<p class="text-sm" data-testid="messages-loading">Nachrichten werden geladen …</p>
		{:else if messages.length === 0}
			<p class="py-8 text-center text-sm" style="color: var(--text-secondary);">
				Noch keine Nachrichten. Beginne die Abstimmung zum Tausch.
			</p>
		{:else}
			{#each messages as message (message.id)}
				<div class:ml-auto={message.is_mine} class="max-w-[85%] sm:max-w-[70%]">
					<div
						class="rounded-xl px-4 py-3 text-sm break-words whitespace-pre-wrap"
						style={message.is_mine
							? 'background: var(--color-brand-700); color: white;'
							: 'background: var(--glass);'}
					>
						{message.content}
					</div>
					<p class="mt-1 text-xs" style="color: var(--text-tertiary);">
						{new Date(message.created_at).toLocaleString('de-DE')}
						{#if message.is_mine}
							· {message.read_at ? 'Gelesen' : 'Gesendet'}{/if}
					</p>
				</div>
			{/each}
		{/if}
	</div>

	<form
		class="border-t p-4"
		style="border-color: var(--glass-border);"
		onsubmit={(event) => {
			event.preventDefault();
			void submit();
		}}
	>
		<label for="message-{threadId}" class="mb-2 block text-sm font-semibold">Nachricht</label>
		<textarea
			id="message-{threadId}"
			bind:value={content}
			rows="3"
			maxlength="4000"
			class="w-full resize-y rounded-lg p-3 text-sm"
			style="background: var(--glass); border: 1px solid var(--glass-border);"
			placeholder="Versand, Zustand oder andere Details abstimmen …"></textarea>
		<div class="mt-2 flex items-center justify-between gap-3">
			<span class="text-xs" style="color: var(--text-tertiary);">{content.length}/4000</span>
			<button
				type="submit"
				disabled={sending || content.trim().length === 0 || content.length > 4000}
				class="cursor-pointer rounded-lg px-4 py-2 text-sm font-semibold disabled:cursor-not-allowed disabled:opacity-50"
				style="background: var(--color-brand-500); color: #000;"
			>
				{sending ? 'Wird gesendet …' : 'Senden'}
			</button>
		</div>
		{#if error}<p class="mt-2 text-sm" role="alert" style="color: var(--color-error);">
				{error}
			</p>{/if}
	</form>
	<p class="sr-only" aria-live="polite">{announcement}</p>
</section>
