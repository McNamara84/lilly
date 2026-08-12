<script lang="ts">
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import {
		cancelOAuthLink,
		confirmOAuthLink,
		fetchPendingOAuthLink,
		type PendingOAuthLink
	} from '$lib/api/auth';
	import { getAuthState } from '$lib/stores/auth.svelte';

	const auth = getAuthState();
	let pending = $state<PendingOAuthLink | null>(null);
	let loading = $state(true);
	let saving = $state(false);
	let error = $state('');

	$effect(() => {
		void loadPendingLink();
	});

	async function loadPendingLink() {
		loading = true;
		try {
			pending = await fetchPendingOAuthLink();
			if (!pending.pending) error = 'Die Verknüpfungsanfrage ist abgelaufen oder nicht vorhanden.';
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'Verknüpfung konnte nicht geladen werden.';
		} finally {
			loading = false;
		}
	}

	async function confirmLink() {
		if (!auth.isAuthenticated) return;
		const confirmationToken = pending?.confirmation_token;
		if (!confirmationToken) {
			error = 'Die Verknüpfungsanfrage ist abgelaufen oder nicht vorhanden.';
			return;
		}
		saving = true;
		error = '';
		try {
			await confirmOAuthLink(confirmationToken);
			await goto(resolve('/profile'));
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'Verknüpfung ist fehlgeschlagen.';
		} finally {
			saving = false;
		}
	}

	async function cancelLink() {
		saving = true;
		try {
			await cancelOAuthLink();
			await goto(resolve(auth.isAuthenticated ? '/profile' : '/login'));
		} catch (cause) {
			error =
				cause instanceof Error ? cause.message : 'Verknüpfung konnte nicht abgebrochen werden.';
		} finally {
			saving = false;
		}
	}

	function providerName(provider?: string): string {
		return provider === 'github' ? 'GitHub' : provider === 'google' ? 'Google' : 'OAuth';
	}
</script>

<svelte:head>
	<title>Konto verknüpfen – LILLY</title>
</svelte:head>

<div class="min-h-[calc(100vh-3.5rem)] px-4 py-10">
	<section class="glass-elevated mx-auto max-w-lg rounded-lg p-6" aria-labelledby="link-heading">
		<h1 id="link-heading" class="mb-3 text-2xl font-bold">Konto sicher verknüpfen</h1>

		{#if loading}
			<p data-testid="oauth-link-loading">Verknüpfung wird geladen …</p>
		{:else if pending?.pending}
			<p class="mb-4 text-sm" style="color: var(--text-secondary);">
				Die bestätigte {providerName(pending.provider)}-Adresse
				<strong data-testid="oauth-link-email">{pending.masked_email}</strong> gehört bereits zu einem
				LILLY-Konto. Die Konten werden nicht automatisch verbunden.
			</p>

			{#if auth.isLoading}
				<p>Kontostatus wird geprüft …</p>
			{:else if !auth.isAuthenticated}
				<p class="mb-5 text-sm">
					Melde dich zuerst mit einem bereits eingerichteten Anmeldeverfahren bei diesem Konto an.
				</p>
				<!-- eslint-disable svelte/no-navigation-without-resolve -- query params appended to resolved route -->
				<a
					href={`${resolve('/login')}?return_to=%2Foauth%2Flink`}
					class="inline-flex rounded-lg px-4 py-2 font-semibold text-white"
					style="background: var(--color-brand-700);"
					data-testid="oauth-link-login"
				>
					Beim bestehenden Konto anmelden
				</a>
				<!-- eslint-enable svelte/no-navigation-without-resolve -->
			{:else}
				<p class="mb-5 text-sm">
					Du bist als <strong>{auth.user?.display_name}</strong> angemeldet. Bestätige die Verknüpfung
					nur, wenn dieses Konto zu der angezeigten Adresse gehört.
				</p>
				<button
					type="button"
					disabled={saving}
					onclick={confirmLink}
					class="rounded-lg px-4 py-2 font-semibold text-white disabled:opacity-50"
					style="background: var(--color-brand-700);"
					data-testid="oauth-link-confirm"
				>
					{saving ? 'Wird verknüpft …' : `${providerName(pending.provider)} verknüpfen`}
				</button>
			{/if}

			<button
				type="button"
				disabled={saving}
				onclick={cancelLink}
				class="ml-3 rounded-lg px-4 py-2 text-sm disabled:opacity-50"
				data-testid="oauth-link-cancel"
			>
				Abbrechen
			</button>
		{/if}

		<div class="mt-4 min-h-6" aria-live="polite">
			{#if error}
				<p role="alert" style="color: var(--color-error);" data-testid="oauth-link-error">
					{error}
				</p>
			{/if}
		</div>
	</section>
</div>
