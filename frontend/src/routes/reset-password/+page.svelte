<script lang="ts">
	import PasswordStrengthMeter from '$lib/components/auth/PasswordStrengthMeter.svelte';
	import { confirmPasswordReset } from '$lib/api/auth';
	import { checkPasswordStrength, MIN_SCORE } from '$lib/utils/password-strength';
	import { goto, replaceState } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { page } from '$app/state';

	let password = $state('');
	let passwordConfirmation = $state('');
	let passwordTouched = $state(false);
	let confirmationTouched = $state(false);
	let isLoading = $state(false);
	let errorMessage = $state('');
	let serverFieldErrors = $state<Record<string, string>>({});
	let token = $derived(page.url.searchParams.get('token') ?? '');
	let tokenAvailable = $derived(token.length >= 43 && token.length <= 128);
	let strength = $derived(checkPasswordStrength(password));
	let passwordError = $derived(
		serverFieldErrors.password ||
			(passwordTouched && !password
				? 'Passwort ist erforderlich'
				: passwordTouched && password.length < 8
					? 'Passwort muss mindestens 8 Zeichen lang sein'
					: passwordTouched && password.length > 128
						? 'Passwort darf höchstens 128 Zeichen lang sein'
						: passwordTouched && strength.score < MIN_SCORE
							? 'Passwort ist zu schwach'
							: '')
	);
	let passwordDescription = $derived(
		password
			? `reset-password-strength${passwordError ? ' new-password-error' : ''}`
			: passwordError
				? 'new-password-error'
				: undefined
	);
	let confirmationError = $derived(
		serverFieldErrors.password_confirmation ||
			(confirmationTouched && !passwordConfirmation
				? 'Passwortbestätigung ist erforderlich'
				: confirmationTouched && passwordConfirmation !== password
					? 'Passwörter stimmen nicht überein'
					: '')
	);
	let formValid = $derived(
		tokenAvailable &&
			password.length >= 8 &&
			password.length <= 128 &&
			strength.score >= MIN_SCORE &&
			passwordConfirmation === password
	);

	async function handleSubmit(event: SubmitEvent) {
		event.preventDefault();
		passwordTouched = true;
		confirmationTouched = true;
		if (!formValid) return;
		isLoading = true;
		errorMessage = '';
		serverFieldErrors = {};
		try {
			await confirmPasswordReset({
				token,
				password,
				password_confirmation: passwordConfirmation
			});
			replaceState(resolve('/reset-password'), {});
			// eslint-disable-next-line svelte/no-navigation-without-resolve -- query appended to resolved path
			await goto(`${resolve('/login')}?reset=true`);
		} catch (cause) {
			const error = cause as Error & {
				code?: string;
				fields?: Record<string, string>;
				retry_after_seconds?: number;
			};
			serverFieldErrors = error.fields ?? {};
			if (error.code === 'PASSWORD_RESET_TOKEN_INVALID') {
				errorMessage = 'Der Reset-Link ist ungültig, abgelaufen oder wurde bereits verwendet.';
			} else if (error.retry_after_seconds) {
				errorMessage = `Zu viele Versuche. Bitte warte ${error.retry_after_seconds} Sekunden.`;
			} else {
				errorMessage = error.fields
					? 'Bitte überprüfe die markierten Felder.'
					: error.message || 'Das Passwort konnte nicht geändert werden.';
			}
		} finally {
			isLoading = false;
		}
	}
</script>

<svelte:head>
	<title>Passwort zurücksetzen – LILLY</title>
	<meta name="referrer" content="no-referrer" />
</svelte:head>

<main class="min-h-[calc(100vh-3.5rem)] flex items-center justify-center px-4 py-8 auth-background">
	<section class="glass-elevated w-full max-w-[440px] p-8" aria-labelledby="reset-title">
		<h1 id="reset-title" class="text-3xl font-bold mb-3" style="color: var(--color-brand-700);">
			Neues Passwort festlegen
		</h1>
		{#if !tokenAvailable}
			<div
				role="alert"
				class="rounded-lg px-4 py-3 text-sm mb-5"
				style="background-color: rgba(239, 68, 68, 0.1); border: 1px solid rgba(239, 68, 68, 0.3); color: #dc2626;"
				data-testid="missing-reset-token"
			>
				Dieser Reset-Link ist unvollständig oder ungültig. Fordere bitte einen neuen Link an.
			</div>
			<a
				href={resolve('/forgot-password')}
				class="font-medium text-sm"
				style="color: var(--color-brand-500);">Neuen Reset-Link anfordern</a
			>
		{:else}
			<p class="text-sm mb-6" style="color: var(--text-secondary);">
				Wähle ein starkes neues Passwort. Nach der Änderung musst du dich auf allen Geräten neu
				anmelden.
			</p>
			{#if errorMessage}
				<div
					role="alert"
					class="rounded-lg px-4 py-3 text-sm mb-4"
					style="background-color: rgba(239, 68, 68, 0.1); border: 1px solid rgba(239, 68, 68, 0.3); color: #dc2626;"
					data-testid="reset-confirm-error"
				>
					{errorMessage}
				</div>
			{/if}
			<form onsubmit={handleSubmit} novalidate class="space-y-5">
				<div>
					<label for="new-password" class="block text-sm font-medium mb-1.5">
						Neues Passwort
					</label>
					<input
						id="new-password"
						data-testid="new-password-input"
						type="password"
						bind:value={password}
						oninput={() => (serverFieldErrors = { ...serverFieldErrors, password: '' })}
						onblur={() => (passwordTouched = true)}
						autocomplete="new-password"
						class="w-full rounded-lg px-4 py-2.5 text-sm outline-none"
						style="background-color: var(--surface-raised); border: 1px solid var(--glass-border); color: var(--text-primary);"
						aria-invalid={passwordError ? 'true' : undefined}
						aria-describedby={passwordDescription}
					/>
					{#if password}
						<PasswordStrengthMeter {strength} id="reset-password-strength" />
					{/if}
					{#if passwordError}
						<p id="new-password-error" class="mt-1 text-xs" style="color: #dc2626;">
							{passwordError}
						</p>
					{/if}
				</div>
				<div>
					<label for="new-password-confirmation" class="block text-sm font-medium mb-1.5">
						Passwort bestätigen
					</label>
					<input
						id="new-password-confirmation"
						data-testid="new-password-confirmation-input"
						type="password"
						bind:value={passwordConfirmation}
						oninput={() =>
							(serverFieldErrors = { ...serverFieldErrors, password_confirmation: '' })}
						onblur={() => (confirmationTouched = true)}
						autocomplete="new-password"
						class="w-full rounded-lg px-4 py-2.5 text-sm outline-none"
						style="background-color: var(--surface-raised); border: 1px solid var(--glass-border); color: var(--text-primary);"
						aria-invalid={confirmationError ? 'true' : undefined}
						aria-describedby={confirmationError ? 'new-password-confirmation-error' : undefined}
					/>
					{#if confirmationError}
						<p id="new-password-confirmation-error" class="mt-1 text-xs" style="color: #dc2626;">
							{confirmationError}
						</p>
					{/if}
				</div>
				<button
					type="submit"
					disabled={isLoading}
					class="w-full rounded-lg px-4 py-2.5 text-sm font-semibold text-white disabled:opacity-50"
					style="background-color: var(--color-brand-700);"
					data-testid="reset-confirm-submit"
				>
					{isLoading ? 'Wird gespeichert …' : 'Passwort ändern'}
				</button>
			</form>
		{/if}
	</section>
</main>

<style>
	.auth-background {
		background:
			radial-gradient(ellipse at 20% 50%, rgba(6, 182, 212, 0.08) 0%, transparent 50%),
			var(--surface-base);
	}
</style>
