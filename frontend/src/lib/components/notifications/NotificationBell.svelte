<script lang="ts">
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import {
		fetchNotifications,
		fetchUnreadNotificationCount,
		markAllNotificationsRead,
		markNotificationRead,
		type AppNotification
	} from '$lib/api/notifications';

	let count = $state(0);
	let notifications = $state<AppNotification[]>([]);
	let open = $state(false);
	let loading = $state(false);
	let error = $state<string | null>(null);

	function label(kind: AppNotification['kind']): string {
		return {
			trade_match: 'Neuer Tausch-Match',
			trade_match_updated: 'Tausch-Match aktualisiert',
			trade_proposed: 'Neuer Tauschvorschlag',
			trade_accepted: 'Tauschvorschlag angenommen',
			trade_cancelled: 'Tausch abgebrochen',
			trade_message: 'Neue Nachricht'
		}[kind];
	}

	async function refresh() {
		try {
			count = await fetchUnreadNotificationCount();
		} catch {
			// A temporary polling error must not disturb the rest of the navigation.
		}
	}

	async function toggle() {
		open = !open;
		if (!open) return;
		loading = true;
		error = null;
		try {
			notifications = (await fetchNotifications({ per_page: 10 })).data;
		} catch (cause) {
			error =
				cause instanceof Error ? cause.message : 'Benachrichtigungen konnten nicht geladen werden.';
		} finally {
			loading = false;
		}
	}

	async function openNotification(notification: AppNotification) {
		if (!notification.read_at) {
			await markNotificationRead(notification.id);
			count = Math.max(0, count - 1);
		}
		open = false;
		const threadId = notification.payload.thread_id;
		if (notification.kind === 'trade_message' && typeof threadId === 'number') {
			await goto(resolve('/messages/[id]', { id: String(threadId) }));
		} else if (notification.trade_id) {
			await goto(resolve('/trades/[id]', { id: String(notification.trade_id) }));
		} else {
			await goto(resolve('/trades'));
		}
	}

	async function markAll() {
		await markAllNotificationsRead();
		count = 0;
		notifications = notifications.map((notification) => ({
			...notification,
			read_at: notification.read_at ?? new Date().toISOString()
		}));
	}

	$effect(() => {
		void refresh();
		const interval = window.setInterval(() => {
			if (!document.hidden) void refresh();
		}, 30_000);
		const onFocus = () => void refresh();
		window.addEventListener('focus', onFocus);
		return () => {
			window.clearInterval(interval);
			window.removeEventListener('focus', onFocus);
		};
	});
</script>

<div class="relative">
	<button
		type="button"
		onclick={toggle}
		class="relative cursor-pointer rounded-lg p-2"
		style="background: var(--glass);"
		aria-label={`Benachrichtigungen${count ? `, ${count} ungelesen` : ''}`}
		aria-expanded={open}
	>
		<span aria-hidden="true">🔔</span>
		{#if count > 0}
			<span
				class="absolute -right-1 -top-1 min-w-5 rounded-full px-1 text-center text-xs font-bold"
				style="background: var(--color-error); color: white;"
				data-testid="notification-count"
			>
				{count > 99 ? '99+' : count}
			</span>
		{/if}
	</button>

	{#if open}
		<div
			class="glass-elevated absolute right-0 top-12 z-50 w-80 max-w-[calc(100vw-2rem)] rounded-xl p-3 shadow-xl"
			data-testid="notification-popover"
		>
			<header class="mb-2 flex items-center justify-between gap-2">
				<h2 class="font-semibold">Benachrichtigungen</h2>
				{#if count > 0}
					<button type="button" class="cursor-pointer text-xs underline" onclick={markAll}>
						Alle gelesen
					</button>
				{/if}
			</header>
			{#if error}
				<p class="p-3 text-sm" role="alert" style="color: var(--color-error);">{error}</p>
			{:else if loading}
				<p class="p-3 text-sm">Wird geladen …</p>
			{:else if notifications.length === 0}
				<p class="p-3 text-sm" style="color: var(--text-secondary);">Keine Benachrichtigungen.</p>
			{:else}
				<div class="max-h-80 space-y-1 overflow-y-auto">
					{#each notifications as notification (notification.id)}
						<button
							type="button"
							onclick={() => openNotification(notification)}
							class="w-full cursor-pointer rounded-lg p-3 text-left text-sm"
							style={notification.read_at ? '' : 'background: var(--glass);'}
						>
							<span class="block font-medium">{label(notification.kind)}</span>
							<span class="block text-xs" style="color: var(--text-tertiary);">
								{new Date(notification.created_at).toLocaleString('de-DE')}
							</span>
						</button>
					{/each}
				</div>
			{/if}
		</div>
	{/if}
</div>
