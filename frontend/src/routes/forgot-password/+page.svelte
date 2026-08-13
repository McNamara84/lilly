<script lang="ts">
	import { requestPasswordReset } from '$lib/api/auth';
	import { resolve } from '$app/paths';

	let email = $state('');
	let emailTouched = $state(false);
	let isLoading = $state(false);
	let submitted = $state(false);
	let errorMessage = $state('');
	let retryAfterSeconds = $state<number | null>(null);
	let trimmedEmail = $derived(email.trim());
	let emailError = $derived(
		emailTouched && !trimmedEmail
			? 'E-Mail-Adresse ist erforderlich'
			: emailTouched && !/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(trimmedEmail)
				? 'Bitte eine gültige E-Mail-Adresse eingeben'
				: ''
	);

	async function handleSubmit(event: SubmitEvent) {
		event.preventDefault();
		emailTouched = true;
		if (!trimmedEmail || !/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(trimmedEmail)) return;
		isLoading = true;
		errorMessage = '';
		retryAfterSeconds = null;
		try {
			await requestPasswordReset(trimmedEmail);
			submitted = true;
		} catch (cause) {
			const error = cause as Error & { retry_after_seconds?: number };
			if (error.retry_after_seconds) {
				retryAfterSeconds = error.retry_after_seconds;
				errorMessage = `Zu viele Anfragen. Bitte versuche es in ${error.retry_after_seconds} Sekunden erneut.`;
			} else {
				errorMessage = error.message || 'Die Anfrage konnte nicht verarbeitet werden.';
			}
		} finally {
			isLoading = false;
		}
	}
</script>

<svelte:head>
	<title>Passwort vergessen – LILLY</title>
</svelte:head>

<main class="min-h-[calc(100vh-3.5rem)] flex items-center justify-center px-4 py-8 auth-background">
	<section class="glass-elevated w-full max-w-[440px] p-8" aria-labelledby="forgot-title">
		<h1 id="forgot-title" class="text-3xl font-bold mb-3" style="color: var(--color-brand-700);">
			Passwort vergessen?
		</h1>
		{#if submitted}
			<div
				role="status"
				class="rounded-lg px-4 py-3 text-sm mb-5"
				style="background-color: rgba(34, 197, 94, 0.1); border: 1px solid rgba(34, 197, 94, 0.3); color: #15803d;"
				data-testid="reset-request-success"
			>
				Falls für diese Adresse ein berechtigtes Konto existiert, haben wir eine E-Mail mit einem
				Link zum Zurücksetzen versendet.
			</div>
			<p class="text-sm mb-6" style="color: var(--text-secondary);">
				Prüfe bitte auch deinen Spam-Ordner. Der Link ist zeitlich begrenzt und nur einmal
				verwendbar.
			</p>
		{:else}
			<p class="text-sm mb-6" style="color: var(--text-secondary);">
				Gib deine E-Mail-Adresse ein. Wenn ein Passwortkonto existiert, senden wir dir einen
				sicheren Link.
			</p>
			{#if errorMessage}
				<div
					role="alert"
					class="rounded-lg px-4 py-3 text-sm mb-4"
					style="background-color: rgba(239, 68, 68, 0.1); border: 1px solid rgba(239, 68, 68, 0.3); color: #dc2626;"
					data-testid="reset-request-error"
					data-retry-after={retryAfterSeconds}
				>
					{errorMessage}
				</div>
			{/if}
			<form onsubmit={handleSubmit} novalidate class="space-y-5">
				<div>
					<label for="reset-email" class="block text-sm font-medium mb-1.5">E-Mail</label>
					<input
						id="reset-email"
						data-testid="reset-email-input"
						type="email"
						bind:value={email}
						onblur={() => (emailTouched = true)}
						autocomplete="email"
						class="w-full rounded-lg px-4 py-2.5 text-sm outline-none"
						style="background-color: var(--surface-raised); border: 1px solid var(--glass-border); color: var(--text-primary);"
						aria-invalid={emailError ? 'true' : undefined}
						aria-describedby={emailError ? 'reset-email-error' : undefined}
					/>
					{#if emailError}
						<p id="reset-email-error" class="mt-1 text-xs" style="color: #dc2626;">
							{emailError}
						</p>
					{/if}
				</div>
				<button
					type="submit"
					disabled={isLoading}
					class="w-full rounded-lg px-4 py-2.5 text-sm font-semibold text-white disabled:opacity-50"
					style="background-color: var(--color-brand-700);"
					data-testid="reset-request-submit"
				>
					{isLoading ? 'Wird gesendet …' : 'Reset-Link anfordern'}
				</button>
			</form>
		{/if}
		<p class="mt-6 text-center text-sm">
			<a href={resolve('/login')} class="font-medium" style="color: var(--color-brand-500);">
				Zurück zur Anmeldung
			</a>
		</p>
	</section>
</main>

<style>
	.auth-background {
		background:
			radial-gradient(ellipse at 20% 50%, rgba(6, 182, 212, 0.08) 0%, transparent 50%),
			var(--surface-base);
	}
</style>
