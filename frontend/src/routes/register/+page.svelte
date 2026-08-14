<script lang="ts">
	import {
		fetchAuthOptions,
		register,
		startOAuth,
		type AuthOptionsResponse,
		type OAuthProvider
	} from '$lib/api/auth';
	import { checkPasswordStrength, MIN_SCORE } from '$lib/utils/password-strength';
	import PasswordStrengthMeter from '$lib/components/auth/PasswordStrengthMeter.svelte';
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { page } from '$app/state';

	let displayName = $state('');
	let email = $state('');
	let password = $state('');
	let passwordConfirmation = $state('');
	let privacyConsent = $state(false);
	let errorMessage = $state(oauthErrorMessage(page.url.searchParams.get('oauth_error')));
	let isLoading = $state(false);
	let oauthLoading = $state<OAuthProvider | null>(null);
	let authOptions = $state<AuthOptionsResponse | null>(null);
	let serverFieldErrors = $state<Record<string, string>>({});

	let displayNameTouched = $state(false);
	let emailTouched = $state(false);
	let passwordTouched = $state(false);
	let passwordConfirmationTouched = $state(false);

	let trimmedDisplayName = $derived(displayName.trim());
	let trimmedEmail = $derived(email.trim());
	let privacyPolicyVersion = $derived(authOptions?.privacy_policy.version ?? '');

	$effect(() => {
		void loadAuthOptions();
	});

	async function loadAuthOptions() {
		try {
			authOptions = await fetchAuthOptions();
		} catch (cause) {
			errorMessage =
				cause instanceof Error
					? cause.message
					: 'Registrierungsoptionen konnten nicht geladen werden.';
		}
	}

	let passwordStrength = $derived(
		checkPasswordStrength(password, [trimmedDisplayName, trimmedEmail])
	);

	let displayNameError = $derived(
		serverFieldErrors.display_name ||
			(displayNameTouched && !trimmedDisplayName ? 'Anzeigename ist erforderlich' : '')
	);

	let emailError = $derived(
		serverFieldErrors.email ||
			(emailTouched && !trimmedEmail
				? 'E-Mail-Adresse ist erforderlich'
				: emailTouched && !/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(trimmedEmail)
					? 'Bitte eine gültige E-Mail-Adresse eingeben'
					: '')
	);

	let passwordError = $derived(
		serverFieldErrors.password ||
			(passwordTouched && !password
				? 'Passwort ist erforderlich'
				: passwordTouched && password.length < 8
					? 'Passwort muss mindestens 8 Zeichen lang sein'
					: passwordTouched && passwordStrength.score < MIN_SCORE
						? 'Passwort ist zu schwach'
						: '')
	);

	let passwordConfirmationError = $derived(
		serverFieldErrors.password_confirmation ||
			(passwordConfirmationTouched && !passwordConfirmation
				? 'Passwortbestätigung ist erforderlich'
				: passwordConfirmationTouched && passwordConfirmation !== password
					? 'Passwörter stimmen nicht überein'
					: '')
	);

	let privacyConsentError = $derived(serverFieldErrors.privacy_consent || '');

	let isFormValid = $derived(
		trimmedDisplayName !== '' &&
			trimmedEmail !== '' &&
			/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(trimmedEmail) &&
			password.length >= 8 &&
			passwordStrength.score >= MIN_SCORE &&
			passwordConfirmation === password &&
			privacyConsent &&
			privacyPolicyVersion !== ''
	);

	async function handleSubmit(event: SubmitEvent) {
		event.preventDefault();
		displayNameTouched = true;
		emailTouched = true;
		passwordTouched = true;
		passwordConfirmationTouched = true;

		if (!isFormValid) return;

		isLoading = true;
		errorMessage = '';
		serverFieldErrors = {};

		try {
			await register({
				display_name: trimmedDisplayName,
				email: trimmedEmail,
				password,
				password_confirmation: passwordConfirmation,
				privacy_consent: privacyConsent,
				privacy_policy_version: privacyPolicyVersion
			});
			// eslint-disable-next-line svelte/no-navigation-without-resolve -- query params appended to resolved route
			await goto(`${resolve('/login')}?registered=true`);
		} catch (err) {
			const apiError = err as Error & { code?: string; fields?: Record<string, string> };
			if (apiError.code === 'PRIVACY_POLICY_CHANGED') {
				privacyConsent = false;
				await loadAuthOptions();
			}
			serverFieldErrors = apiError.fields ?? {};
			errorMessage = apiError.fields
				? 'Bitte überprüfe die markierten Felder.'
				: err instanceof Error
					? err.message
					: 'Ein unerwarteter Fehler ist aufgetreten';
		} finally {
			isLoading = false;
		}
	}

	async function handleOAuth(provider: OAuthProvider) {
		if (!privacyConsent || !privacyPolicyVersion || !authOptions?.oauth[provider]) return;
		oauthLoading = provider;
		errorMessage = '';
		try {
			const authorizationUrl = await startOAuth(provider, 'register', {
				privacy_consent: true,
				privacy_policy_version: privacyPolicyVersion
			});
			window.location.assign(authorizationUrl);
		} catch (cause) {
			if ((cause as Error & { code?: string }).code === 'PRIVACY_POLICY_CHANGED') {
				privacyConsent = false;
				await loadAuthOptions();
			}
			errorMessage =
				cause instanceof Error ? cause.message : 'OAuth konnte nicht gestartet werden.';
			oauthLoading = null;
		}
	}

	function oauthErrorMessage(code: string | null): string {
		switch (code) {
			case 'PRIVACY_POLICY_CHANGED':
				return 'Die Datenschutzerklärung wurde geändert. Bitte lies sie erneut und stimme der aktuellen Version zu.';
			case 'PRIVACY_CONSENT_REQUIRED':
				return 'Für die Registrierung ist deine ausdrückliche Datenschutz-Einwilligung erforderlich.';
			case 'OAUTH_PROVIDER_DENIED':
				return 'Die Registrierung beim Anbieter wurde abgebrochen.';
			case 'OAUTH_VERIFIED_EMAIL_REQUIRED':
				return 'Der Anbieter hat keine bestätigte primäre E-Mail-Adresse bereitgestellt.';
			case 'OAUTH_STATE_INVALID':
				return 'Der Registrierungsvorgang ist abgelaufen oder ungültig. Bitte versuche es erneut.';
			case 'OAUTH_PROVIDER_DISABLED':
				return 'Dieser Registrierungsanbieter ist derzeit nicht verfügbar.';
			case 'OAUTH_PROVIDER_ERROR':
				return 'Der Registrierungsanbieter konnte nicht erreicht werden. Bitte versuche es erneut.';
			default:
				return '';
		}
	}
</script>

<svelte:head>
	<title>Registrieren – LILLY</title>
</svelte:head>

<div
	class="min-h-[calc(100vh-3.5rem)] flex items-center justify-center px-4 py-8 register-background"
>
	<div class="glass-elevated w-full max-w-[420px] p-8">
		<!-- Logo & Tagline -->
		<div class="text-center mb-8">
			<h1 class="text-4xl font-bold mb-2" style="color: var(--color-brand-700);">LILLY</h1>
			<p style="color: var(--text-secondary);" class="text-sm">
				Listing Inventory for Lovely Little Yellowbacks
			</p>
		</div>

		<!-- Register Form -->
		<form onsubmit={handleSubmit} class="space-y-4" novalidate data-testid="register-form">
			{#if errorMessage}
				<div
					class="rounded-lg px-4 py-3 text-sm"
					style="background-color: rgba(239, 68, 68, 0.1); border: 1px solid rgba(239, 68, 68, 0.3); color: #ef4444;"
					role="alert"
					data-testid="register-error"
				>
					{errorMessage}
				</div>
			{/if}

			<!-- Display Name -->
			<div>
				<label
					for="display-name"
					class="block text-sm font-medium mb-1.5"
					style="color: var(--text-secondary);"
				>
					Anzeigename
				</label>
				<input
					id="display-name"
					data-testid="display-name-input"
					type="text"
					bind:value={displayName}
					oninput={() => (serverFieldErrors = { ...serverFieldErrors, display_name: '' })}
					onblur={() => (displayNameTouched = true)}
					placeholder="Max Mustermann"
					autocomplete="name"
					class="w-full rounded-lg px-4 py-2.5 text-sm outline-none transition-colors"
					style="background-color: var(--surface-raised); border: 1px solid var(--glass-border); color: var(--text-primary);"
					aria-invalid={displayNameError ? 'true' : undefined}
					aria-describedby={displayNameError ? 'display-name-error' : undefined}
				/>
				{#if displayNameError}
					<p id="display-name-error" class="mt-1 text-xs" style="color: #ef4444;">
						{displayNameError}
					</p>
				{/if}
			</div>

			<!-- Email -->
			<div>
				<label
					for="email"
					class="block text-sm font-medium mb-1.5"
					style="color: var(--text-secondary);"
				>
					E-Mail
				</label>
				<input
					id="email"
					data-testid="email-input"
					type="email"
					bind:value={email}
					oninput={() => (serverFieldErrors = { ...serverFieldErrors, email: '' })}
					onblur={() => (emailTouched = true)}
					placeholder="name@example.com"
					autocomplete="email"
					class="w-full rounded-lg px-4 py-2.5 text-sm outline-none transition-colors"
					style="background-color: var(--surface-raised); border: 1px solid var(--glass-border); color: var(--text-primary);"
					aria-invalid={emailError ? 'true' : undefined}
					aria-describedby={emailError ? 'email-error' : undefined}
				/>
				{#if emailError}
					<p id="email-error" class="mt-1 text-xs" style="color: #ef4444;">{emailError}</p>
				{/if}
			</div>

			<!-- Password -->
			<div>
				<label
					for="password"
					class="block text-sm font-medium mb-1.5"
					style="color: var(--text-secondary);"
				>
					Passwort
				</label>
				<input
					id="password"
					data-testid="password-input"
					type="password"
					bind:value={password}
					oninput={() => (serverFieldErrors = { ...serverFieldErrors, password: '' })}
					onblur={() => (passwordTouched = true)}
					placeholder="••••••••"
					autocomplete="new-password"
					class="w-full rounded-lg px-4 py-2.5 text-sm outline-none transition-colors"
					style="background-color: var(--surface-raised); border: 1px solid var(--glass-border); color: var(--text-primary);"
					aria-invalid={passwordError ? 'true' : undefined}
					aria-describedby="password-strength-info{passwordError ? ' password-error' : ''}"
				/>
				{#if password}
					<PasswordStrengthMeter strength={passwordStrength} />
				{/if}
				{#if passwordError}
					<p id="password-error" class="mt-1 text-xs" style="color: #ef4444;">
						{passwordError}
					</p>
				{/if}
			</div>

			<!-- Password Confirmation -->
			<div>
				<label
					for="password-confirmation"
					class="block text-sm font-medium mb-1.5"
					style="color: var(--text-secondary);"
				>
					Passwort bestätigen
				</label>
				<input
					id="password-confirmation"
					data-testid="password-confirmation-input"
					type="password"
					bind:value={passwordConfirmation}
					oninput={() => (serverFieldErrors = { ...serverFieldErrors, password_confirmation: '' })}
					onblur={() => (passwordConfirmationTouched = true)}
					placeholder="••••••••"
					autocomplete="new-password"
					class="w-full rounded-lg px-4 py-2.5 text-sm outline-none transition-colors"
					style="background-color: var(--surface-raised); border: 1px solid var(--glass-border); color: var(--text-primary);"
					aria-invalid={passwordConfirmationError ? 'true' : undefined}
					aria-describedby={passwordConfirmationError ? 'password-confirmation-error' : undefined}
				/>
				{#if passwordConfirmationError}
					<p id="password-confirmation-error" class="mt-1 text-xs" style="color: #ef4444;">
						{passwordConfirmationError}
					</p>
				{/if}
			</div>

			<!-- Privacy Consent -->
			<div class="flex items-start gap-3">
				<input
					id="privacy-consent"
					data-testid="privacy-consent-checkbox"
					type="checkbox"
					bind:checked={privacyConsent}
					onchange={() => (serverFieldErrors = { ...serverFieldErrors, privacy_consent: '' })}
					aria-invalid={privacyConsentError ? 'true' : undefined}
					aria-describedby={privacyConsentError ? 'privacy-consent-error' : undefined}
					class="mt-0.5 h-4 w-4 rounded accent-[var(--color-brand-700)]"
				/>
				<label for="privacy-consent" class="text-sm" style="color: var(--text-secondary);">
					Ich stimme der
					<a
						href={resolve('/privacy')}
						class="underline font-medium"
						style="color: var(--color-brand-500);"
						target="_blank"
						rel="noopener noreferrer"
					>
						Datenschutzerklärung{privacyPolicyVersion ? ` (Version ${privacyPolicyVersion})` : ''}
					</a>
					zu.
				</label>
			</div>
			{#if privacyConsentError}
				<p id="privacy-consent-error" class="-mt-2 text-xs" style="color: #ef4444;">
					{privacyConsentError}
				</p>
			{/if}

			<!-- Submit Button -->
			<button
				type="submit"
				data-testid="submit-button"
				disabled={isLoading}
				class="w-full rounded-lg px-4 py-2.5 text-sm font-semibold text-white transition-all cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed"
				style="background-color: var(--color-brand-700);"
			>
				{#if isLoading}
					<span class="inline-flex items-center gap-2">
						<svg
							class="animate-spin h-4 w-4"
							xmlns="http://www.w3.org/2000/svg"
							fill="none"
							viewBox="0 0 24 24"
							aria-hidden="true"
						>
							<circle
								class="opacity-25"
								cx="12"
								cy="12"
								r="10"
								stroke="currentColor"
								stroke-width="4"
							></circle>
							<path
								class="opacity-75"
								fill="currentColor"
								d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z"
							></path>
						</svg>
						Registrieren…
					</span>
				{:else}
					Registrieren
				{/if}
			</button>
		</form>

		<!-- Divider -->
		<div class="flex items-center gap-3 my-6">
			<div class="flex-1 h-px" style="background-color: var(--glass-border);"></div>
			<span class="text-xs" style="color: var(--text-tertiary);">oder weiter mit</span>
			<div class="flex-1 h-px" style="background-color: var(--glass-border);"></div>
		</div>

		<!-- OAuth Buttons -->
		<div class="grid grid-cols-2 gap-3">
			<button
				type="button"
				disabled={!privacyConsent || !authOptions?.oauth.google || oauthLoading !== null}
				onclick={() => handleOAuth('google')}
				class="flex items-center justify-center gap-2 rounded-lg px-4 py-2.5 text-sm font-medium transition-colors cursor-pointer disabled:cursor-not-allowed disabled:opacity-50"
				style="background-color: var(--surface-raised); border: 1px solid var(--glass-border); color: var(--text-secondary);"
				title={authOptions?.oauth.google
					? privacyConsent
						? 'Mit Google registrieren'
						: 'Bitte zuerst der Datenschutzerklärung zustimmen'
					: 'Google-Registrierung ist nicht konfiguriert'}
				data-testid="oauth-google"
			>
				<svg width="16" height="16" viewBox="0 0 24 24" aria-hidden="true">
					<path
						fill="#4285F4"
						d="M22.56 12.25c0-.78-.07-1.53-.2-2.25H12v4.26h5.92a5.06 5.06 0 0 1-2.2 3.32v2.77h3.57c2.08-1.92 3.28-4.74 3.28-8.1z"
					/>
					<path
						fill="#34A853"
						d="M12 23c2.97 0 5.46-.98 7.28-2.66l-3.57-2.77c-.98.66-2.23 1.06-3.71 1.06-2.86 0-5.29-1.93-6.16-4.53H2.18v2.84C3.99 20.53 7.7 23 12 23z"
					/>
					<path
						fill="#FBBC05"
						d="M5.84 14.09c-.22-.66-.35-1.36-.35-2.09s.13-1.43.35-2.09V7.07H2.18C1.43 8.55 1 10.22 1 12s.43 3.45 1.18 4.93l2.85-2.22.81-.62z"
					/>
					<path
						fill="#EA4335"
						d="M12 5.38c1.62 0 3.06.56 4.21 1.64l3.15-3.15C17.45 2.09 14.97 1 12 1 7.7 1 3.99 3.47 2.18 7.07l3.66 2.84c.87-2.6 3.3-4.53 6.16-4.53z"
					/>
				</svg>
				{oauthLoading === 'google' ? 'Weiterleitung …' : 'Google'}
			</button>
			<button
				type="button"
				disabled={!privacyConsent || !authOptions?.oauth.github || oauthLoading !== null}
				onclick={() => handleOAuth('github')}
				class="flex items-center justify-center gap-2 rounded-lg px-4 py-2.5 text-sm font-medium transition-colors cursor-pointer disabled:cursor-not-allowed disabled:opacity-50"
				style="background-color: var(--surface-raised); border: 1px solid var(--glass-border); color: var(--text-secondary);"
				title={authOptions?.oauth.github
					? privacyConsent
						? 'Mit GitHub registrieren'
						: 'Bitte zuerst der Datenschutzerklärung zustimmen'
					: 'GitHub-Registrierung ist nicht konfiguriert'}
				data-testid="oauth-github"
			>
				<svg width="16" height="16" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
					<path
						d="M12 0c-6.626 0-12 5.373-12 12 0 5.302 3.438 9.8 8.207 11.387.599.111.793-.261.793-.577v-2.234c-3.338.726-4.033-1.416-4.033-1.416-.546-1.387-1.333-1.756-1.333-1.756-1.089-.745.083-.729.083-.729 1.205.084 1.839 1.237 1.839 1.237 1.07 1.834 2.807 1.304 3.492.997.107-.775.418-1.305.762-1.604-2.665-.305-5.467-1.334-5.467-5.931 0-1.311.469-2.381 1.236-3.221-.124-.303-.535-1.524.117-3.176 0 0 1.008-.322 3.301 1.23.957-.266 1.983-.399 3.003-.404 1.02.005 2.047.138 3.006.404 2.291-1.552 3.297-1.23 3.297-1.23.653 1.653.242 2.874.118 3.176.77.84 1.235 1.911 1.235 3.221 0 4.609-2.807 5.624-5.479 5.921.43.372.823 1.102.823 2.222v3.293c0 .319.192.694.801.576 4.765-1.589 8.199-6.086 8.199-11.386 0-6.627-5.373-12-12-12z"
					/>
				</svg>
				{oauthLoading === 'github' ? 'Weiterleitung …' : 'GitHub'}
			</button>
		</div>

		<!-- Links -->
		<div class="mt-6 text-center">
			<p class="text-sm" style="color: var(--text-secondary);">
				Bereits ein Konto?
				<a href={resolve('/login')} style="color: var(--color-brand-500);" class="font-medium"
					>Anmelden</a
				>
			</p>
		</div>
	</div>
</div>

<style>
	.register-background {
		background:
			radial-gradient(ellipse at 20% 50%, rgba(6, 182, 212, 0.08) 0%, transparent 50%),
			radial-gradient(ellipse at 80% 20%, rgba(14, 165, 233, 0.06) 0%, transparent 50%),
			radial-gradient(ellipse at 50% 80%, rgba(6, 182, 212, 0.04) 0%, transparent 50%),
			var(--surface-base);
	}
</style>
