<script lang="ts">
	import { login } from '$lib/api/auth';

	let email = $state('');
	let password = $state('');
	let errorMessage = $state('');
	let isLoading = $state(false);
	let emailTouched = $state(false);
	let passwordTouched = $state(false);

	let emailError = $derived(
		emailTouched && !email.trim()
			? 'E-Mail-Adresse ist erforderlich'
			: emailTouched && !/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(email)
				? 'Bitte eine gültige E-Mail-Adresse eingeben'
				: ''
	);

	let passwordError = $derived(
		passwordTouched && !password.trim() ? 'Passwort ist erforderlich' : ''
	);

	let isFormValid = $derived(
		email.trim() !== '' && /^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(email) && password.trim() !== ''
	);

	async function handleSubmit(event: SubmitEvent) {
		event.preventDefault();
		emailTouched = true;
		passwordTouched = true;

		if (!isFormValid) return;

		isLoading = true;
		errorMessage = '';

		try {
			const response = await login({ email, password });
			// TODO: Store token and redirect to dashboard
			void response;
		} catch (err) {
			errorMessage = err instanceof Error ? err.message : 'Ein unerwarteter Fehler ist aufgetreten';
		} finally {
			isLoading = false;
		}
	}
</script>

<svelte:head>
	<title>Anmelden – LILLY</title>
</svelte:head>

<div class="min-h-[calc(100vh-3.5rem)] flex items-center justify-center px-4 py-8 login-background">
	<div class="glass-elevated w-full max-w-[420px] p-8">
		<!-- Logo & Tagline -->
		<div class="text-center mb-8">
			<h1 class="text-4xl font-bold mb-2" style="color: var(--color-brand-700);">LILLY</h1>
			<p style="color: var(--text-secondary);" class="text-sm">
				Listing Inventory for Lovely Little Yellowbacks
			</p>
		</div>

		<!-- Login Form -->
		<form onsubmit={handleSubmit} class="space-y-4" novalidate>
			{#if errorMessage}
				<div
					class="rounded-lg px-4 py-3 text-sm"
					style="background-color: rgba(239, 68, 68, 0.1); border: 1px solid rgba(239, 68, 68, 0.3); color: #ef4444;"
					role="alert"
				>
					{errorMessage}
				</div>
			{/if}

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
					type="email"
					bind:value={email}
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
					type="password"
					bind:value={password}
					onblur={() => (passwordTouched = true)}
					placeholder="••••••••"
					autocomplete="current-password"
					class="w-full rounded-lg px-4 py-2.5 text-sm outline-none transition-colors"
					style="background-color: var(--surface-raised); border: 1px solid var(--glass-border); color: var(--text-primary);"
					aria-invalid={passwordError ? 'true' : undefined}
					aria-describedby={passwordError ? 'password-error' : undefined}
				/>
				{#if passwordError}
					<p id="password-error" class="mt-1 text-xs" style="color: #ef4444;">{passwordError}</p>
				{/if}
			</div>

			<!-- Submit Button -->
			<button
				type="submit"
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
						Anmelden…
					</span>
				{:else}
					Anmelden
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
				disabled
				class="flex items-center justify-center gap-2 rounded-lg px-4 py-2.5 text-sm font-medium transition-colors cursor-not-allowed opacity-50"
				style="background-color: var(--surface-raised); border: 1px solid var(--glass-border); color: var(--text-secondary);"
				title="Google-Login ist noch nicht verfügbar"
			>
				<svg width="16" height="16" viewBox="0 0 24 24">
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
				Google
			</button>
			<button
				type="button"
				disabled
				class="flex items-center justify-center gap-2 rounded-lg px-4 py-2.5 text-sm font-medium transition-colors cursor-not-allowed opacity-50"
				style="background-color: var(--surface-raised); border: 1px solid var(--glass-border); color: var(--text-secondary);"
				title="GitHub-Login ist noch nicht verfügbar"
			>
				<svg width="16" height="16" viewBox="0 0 24 24" fill="currentColor">
					<path
						d="M12 0c-6.626 0-12 5.373-12 12 0 5.302 3.438 9.8 8.207 11.387.599.111.793-.261.793-.577v-2.234c-3.338.726-4.033-1.416-4.033-1.416-.546-1.387-1.333-1.756-1.333-1.756-1.089-.745.083-.729.083-.729 1.205.084 1.839 1.237 1.839 1.237 1.07 1.834 2.807 1.304 3.492.997.107-.775.418-1.305.762-1.604-2.665-.305-5.467-1.334-5.467-5.931 0-1.311.469-2.381 1.236-3.221-.124-.303-.535-1.524.117-3.176 0 0 1.008-.322 3.301 1.23.957-.266 1.983-.399 3.003-.404 1.02.005 2.047.138 3.006.404 2.291-1.552 3.297-1.23 3.297-1.23.653 1.653.242 2.874.118 3.176.77.84 1.235 1.911 1.235 3.221 0 4.609-2.807 5.624-5.479 5.921.43.372.823 1.102.823 2.222v3.293c0 .319.192.694.801.576 4.765-1.589 8.199-6.086 8.199-11.386 0-6.627-5.373-12-12-12z"
					/>
				</svg>
				GitHub
			</button>
		</div>

		<!-- Links -->
		<div class="mt-6 text-center space-y-2">
			<p class="text-sm" style="color: var(--text-tertiary);">Passwort vergessen?</p>
			<p class="text-sm" style="color: var(--text-secondary);">
				Noch kein Konto?
				<span style="color: var(--color-brand-500);" class="font-medium">Registrieren</span>
			</p>
		</div>
	</div>
</div>

<style>
	.login-background {
		background:
			radial-gradient(ellipse at 20% 50%, rgba(6, 182, 212, 0.08) 0%, transparent 50%),
			radial-gradient(ellipse at 80% 20%, rgba(14, 165, 233, 0.06) 0%, transparent 50%),
			radial-gradient(ellipse at 50% 80%, rgba(6, 182, 212, 0.04) 0%, transparent 50%),
			var(--surface-base);
	}
</style>
