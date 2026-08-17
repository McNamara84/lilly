<script lang="ts">
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import {
		cancelAccountDeletion,
		fetchAccountDeletionStatus,
		type AccountDeletionStatus
	} from '$lib/api/account-erasure';
	import { initAuth } from '$lib/stores/auth.svelte';

	let status = $state<AccountDeletionStatus | null>(null);
	let loading = $state(true);
	let cancelling = $state(false);
	let error = $state<string | null>(null);

	$effect(() => {
		void loadStatus();
	});

	async function loadStatus() {
		try {
			status = await fetchAccountDeletionStatus();
		} catch (cause) {
			error =
				cause instanceof Error ? cause.message : 'Der Löschstatus konnte nicht geladen werden.';
		} finally {
			loading = false;
		}
	}

	async function cancelDeletion() {
		cancelling = true;
		error = null;
		try {
			await cancelAccountDeletion();
			await initAuth();
			await goto(resolve('/profile?deletion_cancelled=true'));
		} catch (cause) {
			error =
				cause instanceof Error ? cause.message : 'Die Löschung konnte nicht widerrufen werden.';
			cancelling = false;
		}
	}
</script>

<svelte:head>
	<title>Kontolöschung – LILLY</title>
</svelte:head>

<main class="mx-auto max-w-2xl px-4 py-10 sm:px-6">
	<section class="glass-elevated rounded-xl p-6" aria-labelledby="deletion-status-heading">
		<h1 id="deletion-status-heading" class="text-2xl font-bold">Kontolöschung</h1>

		{#if loading}
			<p class="mt-5">Status wird geladen …</p>
		{:else if status}
			<p class="mt-5">
				Dein Konto ist deaktiviert. Die endgültige Löschung ist für
				<strong>{new Date(status.scheduled_for).toLocaleString('de-DE')}</strong> vorgesehen.
			</p>
			<p class="mt-3 text-sm" style="color: var(--text-secondary);">
				Profil, Sammlung und Matching sind bis dahin nicht sichtbar oder nutzbar. Bereits
				abgebrochene Tausche werden bei einem Widerruf nicht automatisch wieder geöffnet.
			</p>
			{#if status.can_cancel}
				<button
					type="button"
					class="mt-6 rounded-lg px-4 py-2 font-semibold disabled:opacity-50"
					style="background: var(--color-brand-500); color: #000;"
					disabled={cancelling}
					onclick={cancelDeletion}
					data-testid="cancel-account-deletion"
				>
					{cancelling ? 'Widerruf läuft …' : 'Kontolöschung widerrufen'}
				</button>
			{:else}
				<p class="mt-5">Die Widerrufsfrist ist abgelaufen.</p>
			{/if}
		{:else}
			<p class="mt-5">
				Der eingeschränkte Wiederherstellungszugang fehlt oder ist abgelaufen. Melde dich erneut an,
				solange die Widerrufsfrist noch läuft.
			</p>
			<a class="mt-5 inline-block underline" href={resolve('/login')}>Zur Anmeldung</a>
		{/if}

		<div class="mt-4 min-h-6" aria-live="polite">
			{#if error}<p role="alert" style="color: var(--color-error);">{error}</p>{/if}
		</div>
	</section>
</main>
