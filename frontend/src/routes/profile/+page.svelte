<script lang="ts">
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import {
		deleteAvatar,
		fetchOwnProfile,
		updateProfile,
		updateVisibility,
		uploadAvatar,
		type OwnProfile
	} from '$lib/api/profile';
	import {
		fetchPrivacyConsents,
		startOAuth,
		type OAuthProvider,
		type PrivacyConsent
	} from '$lib/api/auth';
	import {
		availableOAuthMethods,
		fetchAccountDeletionOptions,
		requestAccountDeletion,
		reauthenticateWithPassword,
		type AccountDeletionOptions
	} from '$lib/api/account-erasure';
	import { DEFAULT_PHOTO_POLICY, fetchPhotoPolicy, type PhotoPolicy } from '$lib/api/media';
	import { getOfflineStatus } from '$lib/offline/status.svelte';
	import { deactivateAccountLocally, getAuthState, setUser } from '$lib/stores/auth.svelte';

	const auth = getAuthState();
	const offlineStatus = getOfflineStatus();

	let profile = $state<OwnProfile | null>(null);
	let profilePublic = $state(false);
	let collectionPublic = $state(false);
	let displayName = $state('');
	let location = $state('');
	let loading = $state(true);
	let saving = $state(false);
	let profileSaving = $state(false);
	let avatarSaving = $state(false);
	let avatarVersion = $state(0);
	let error = $state<string | null>(null);
	let success = $state<string | null>(null);
	let fieldErrors = $state<Record<string, string>>({});
	let privacyConsents = $state<PrivacyConsent[]>([]);
	let privacyConsentsError = $state<string | null>(null);
	let photoPolicy = $state<PhotoPolicy>(DEFAULT_PHOTO_POLICY);
	let deletionOptions = $state<AccountDeletionOptions | null>(null);
	let deletionOptionsError = $state<string | null>(null);
	let deletionOptionsLoading = $state(false);
	let deletionDialog = $state<HTMLDialogElement>();
	let deletionConfirmation = $state('');
	let deletionPassword = $state('');
	let deleting = $state(false);
	let deletionError = $state<string | null>(null);
	let deletionScheduled = $state(false);
	let oauthReauthLoading = $state<OAuthProvider | null>(null);
	let deletionOffline = $derived(auth.isOfflineSession || !offlineStatus.online);

	$effect(() => {
		if (!auth.isLoading && !auth.isAuthenticated) {
			void goto(resolve('/login'));
		} else if (auth.isAuthenticated && profile === null && loading) {
			void loadProfile();
		}
	});

	async function loadProfile() {
		loading = true;
		error = null;
		privacyConsentsError = null;
		deletionOptionsError = null;
		const [profileResult, consentsResult, policyResult, deletionResult] = await Promise.allSettled([
			fetchOwnProfile(),
			fetchPrivacyConsents(),
			fetchPhotoPolicy(),
			fetchAccountDeletionOptions()
		]);
		if (profileResult.status === 'fulfilled') {
			profile = profileResult.value;
			profilePublic = profile.profile_public;
			collectionPublic = profile.collection_public;
			displayName = profile.display_name;
			location = profile.location ?? '';
		} else {
			const cause = profileResult.reason;
			error = cause instanceof Error ? cause.message : 'Profil konnte nicht geladen werden.';
		}
		if (consentsResult.status === 'fulfilled') {
			privacyConsents = consentsResult.value;
		} else {
			const cause = consentsResult.reason;
			privacyConsentsError =
				cause instanceof Error
					? cause.message
					: 'Datenschutz-Einwilligungen konnten nicht geladen werden.';
		}
		if (policyResult.status === 'fulfilled') photoPolicy = policyResult.value;
		if (deletionResult.status === 'fulfilled') {
			deletionOptions = deletionResult.value;
		} else {
			const cause = deletionResult.reason;
			deletionOptionsError =
				cause instanceof Error ? cause.message : 'Löschoptionen konnten nicht geladen werden.';
		}
		loading = false;
	}

	async function reloadDeletionOptions() {
		deletionOptionsLoading = true;
		deletionOptionsError = null;
		try {
			deletionOptions = await fetchAccountDeletionOptions();
		} catch (cause) {
			deletionOptionsError =
				cause instanceof Error ? cause.message : 'Löschoptionen konnten nicht geladen werden.';
		} finally {
			deletionOptionsLoading = false;
		}
	}

	function validateProfileFields(): boolean {
		const nextErrors: Record<string, string> = {};
		const normalizedName = displayName.trim();
		const normalizedLocation = location.trim();
		if ([...normalizedName].length < 2 || [...normalizedName].length > 100) {
			nextErrors.display_name = 'Der Anzeigename muss 2 bis 100 Zeichen lang sein.';
		}
		if ([...normalizedLocation].length > 255) {
			nextErrors.location = 'Der Standort darf höchstens 255 Zeichen lang sein.';
		}
		fieldErrors = nextErrors;
		return Object.keys(nextErrors).length === 0;
	}

	async function saveProfile(event: SubmitEvent) {
		event.preventDefault();
		error = null;
		success = null;
		if (!validateProfileFields()) return;
		profileSaving = true;
		try {
			const updated = await updateProfile({
				display_name: displayName.trim(),
				location: location.trim() || null
			});
			profile = updated;
			displayName = updated.display_name;
			location = updated.location ?? '';
			if (auth.user) setUser({ ...auth.user, display_name: updated.display_name });
			success = 'Profildaten gespeichert.';
		} catch (cause) {
			if (cause instanceof Error && 'fields' in cause) {
				fieldErrors = (cause as Error & { fields?: Record<string, string> }).fields ?? {};
			}
			error =
				cause instanceof Error ? cause.message : 'Profildaten konnten nicht gespeichert werden.';
		} finally {
			profileSaving = false;
		}
	}

	async function selectAvatar(event: Event) {
		const input = event.currentTarget as HTMLInputElement;
		const file = input.files?.[0];
		input.value = '';
		if (!file) return;
		error = null;
		success = null;
		if (!photoPolicy.allowed_media_types.includes(file.type)) {
			error = 'Bitte wähle ein JPEG-, PNG- oder WebP-Bild.';
			return;
		}
		if (file.size > photoPolicy.max_upload_bytes) {
			error = `Das Bild darf höchstens ${formatBytes(photoPolicy.max_upload_bytes)} groß sein.`;
			return;
		}
		avatarSaving = true;
		try {
			profile = await uploadAvatar(file);
			avatarVersion = Date.now();
			success = 'Avatar gespeichert.';
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'Avatar konnte nicht gespeichert werden.';
		} finally {
			avatarSaving = false;
		}
	}

	async function removeAvatar() {
		avatarSaving = true;
		error = null;
		success = null;
		try {
			await deleteAvatar();
			if (profile) profile = { ...profile, avatar_url: null };
			success = 'Avatar entfernt.';
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'Avatar konnte nicht entfernt werden.';
		} finally {
			avatarSaving = false;
		}
	}

	function initials(name: string): string {
		return name
			.trim()
			.split(/\s+/u)
			.slice(0, 2)
			.map((part) => part[0]?.toUpperCase() ?? '')
			.join('');
	}

	function formatBytes(bytes: number): string {
		return `${(bytes / 1024 / 1024).toLocaleString('de-DE', { maximumFractionDigits: 1 })} MB`;
	}

	function registrationMethodLabel(method: PrivacyConsent['registration_method']): string {
		switch (method) {
			case 'google':
				return 'Google';
			case 'github':
				return 'GitHub';
			case 'legacy':
				return 'Bestehendes Konto';
			default:
				return 'E-Mail und Passwort';
		}
	}

	async function saveVisibility() {
		saving = true;
		error = null;
		success = null;
		try {
			const settings = await updateVisibility({
				profile_public: profilePublic,
				collection_public: collectionPublic
			});
			profilePublic = settings.profile_public;
			collectionPublic = settings.collection_public;
			if (profile) {
				profile.profile_public = profilePublic;
				profile.collection_public = collectionPublic;
			}
			success = 'Sichtbarkeit gespeichert.';
		} catch (cause) {
			error =
				cause instanceof Error ? cause.message : 'Sichtbarkeit konnte nicht gespeichert werden.';
		} finally {
			saving = false;
		}
	}

	function openDeletionDialog() {
		deletionConfirmation = '';
		deletionPassword = '';
		deletionError = null;
		deletionDialog?.showModal();
	}

	async function reauthenticateOAuth(provider: OAuthProvider) {
		oauthReauthLoading = provider;
		deletionError = null;
		try {
			window.location.assign(await startOAuth(provider, 'reauth'));
		} catch (cause) {
			deletionError =
				cause instanceof Error ? cause.message : 'Anmeldung konnte nicht gestartet werden.';
			oauthReauthLoading = null;
		}
	}

	async function deleteAccount(event: SubmitEvent) {
		event.preventDefault();
		if (!deletionOptions || deletionOffline) return;
		deleting = true;
		deletionError = null;
		try {
			if (!deletionScheduled) {
				if (!deletionOptions.recent_authentication) {
					if (!deletionOptions.password) {
						throw new Error(
							'Bitte bestätige deine Anmeldung zuerst mit einem verknüpften Anbieter.'
						);
					}
					await reauthenticateWithPassword(deletionPassword);
				}
				await requestAccountDeletion(deletionConfirmation);
				deletionScheduled = true;
			}
			await deactivateAccountLocally();
			deletionDialog?.close();
			await goto(resolve('/account/deletion'));
		} catch (cause) {
			const apiError = cause as (Error & { code?: string }) | null;
			if (apiError?.code === 'RECENT_AUTH_REQUIRED') {
				await reloadDeletionOptions();
				deletionError =
					'Bitte bestätige deine Anmeldung erneut und versuche es danach noch einmal.';
			} else {
				deletionError =
					cause instanceof Error ? cause.message : 'Das Konto konnte nicht deaktiviert werden.';
			}
			deleting = false;
		}
	}
</script>

<svelte:head>
	<title>Mein Profil – LILLY</title>
</svelte:head>

<div class="min-h-[calc(100vh-3.5rem)] px-4 py-8 sm:px-6 lg:px-8">
	<div class="mx-auto max-w-3xl">
		<h1 class="mb-6 text-2xl font-bold" style="color: var(--text-primary);">Mein Profil</h1>

		{#if loading}
			<p data-testid="profile-loading">Profil wird geladen …</p>
		{:else if profile}
			<section class="glass-elevated mb-6 rounded-lg p-6" aria-labelledby="profile-data-heading">
				<h2 id="profile-data-heading" class="mb-5 text-lg font-semibold">Profildaten</h2>
				<div class="mb-6 flex flex-col gap-5 sm:flex-row sm:items-center">
					<div
						class="flex h-24 w-24 shrink-0 items-center justify-center overflow-hidden rounded-full text-2xl font-bold"
						style="background: var(--surface-raised);"
						data-testid="profile-avatar"
					>
						{#if profile.avatar_url}
							<img
								src={`${profile.avatar_url}?v=${avatarVersion}`}
								alt={`Avatar von ${profile.display_name}`}
								class="h-full w-full object-cover"
							/>
						{:else}
							<span aria-hidden="true">{initials(profile.display_name)}</span>
							<span class="sr-only">Kein Avatar hochgeladen</span>
						{/if}
					</div>
					<div class="space-y-2">
						<p class="font-semibold" data-testid="profile-display-name">{profile.display_name}</p>
						<p class="text-sm" style="color: var(--text-tertiary);">
							JPEG, PNG oder WebP bis {formatBytes(photoPolicy.max_upload_bytes)}
						</p>
						<div class="flex flex-wrap gap-2">
							<label
								for="profile-avatar-input"
								class="cursor-pointer rounded-lg px-3 py-2 text-sm font-semibold"
								style="background: var(--surface-raised);"
							>
								{avatarSaving
									? 'Verarbeitung …'
									: profile.avatar_url
										? 'Avatar ersetzen'
										: 'Avatar wählen'}
							</label>
							<input
								id="profile-avatar-input"
								type="file"
								accept={photoPolicy.allowed_media_types.join(',')}
								capture="user"
								class="sr-only"
								disabled={avatarSaving}
								onchange={selectAvatar}
								data-testid="profile-avatar-input"
							/>
							{#if profile.avatar_url}
								<button
									type="button"
									class="rounded-lg px-3 py-2 text-sm disabled:opacity-50"
									disabled={avatarSaving}
									onclick={removeAvatar}
									data-testid="delete-avatar"
								>
									Avatar entfernen
								</button>
							{/if}
						</div>
					</div>
				</div>

				<form class="grid gap-4 sm:grid-cols-2" onsubmit={saveProfile} novalidate>
					<div>
						<label for="profile-display-name-input" class="mb-1 block text-sm font-medium">
							Anzeigename
						</label>
						<input
							id="profile-display-name-input"
							type="text"
							bind:value={displayName}
							minlength="2"
							required
							disabled={profileSaving}
							aria-invalid={fieldErrors.display_name ? 'true' : undefined}
							aria-describedby={fieldErrors.display_name ? 'display-name-error' : undefined}
							class="w-full rounded-lg border px-3 py-2"
							data-testid="profile-display-name-input"
						/>
						{#if fieldErrors.display_name}
							<p id="display-name-error" class="mt-1 text-sm" style="color: var(--color-error);">
								{fieldErrors.display_name}
							</p>
						{/if}
					</div>
					<div>
						<label for="profile-location-input" class="mb-1 block text-sm font-medium">
							Ort oder Region (optional)
						</label>
						<input
							id="profile-location-input"
							type="text"
							bind:value={location}
							autocomplete="address-level2"
							disabled={profileSaving}
							aria-invalid={fieldErrors.location ? 'true' : undefined}
							aria-describedby={fieldErrors.location
								? 'location-hint location-error'
								: 'location-hint'}
							class="w-full rounded-lg border px-3 py-2"
							data-testid="profile-location-input"
						/>
						<p id="location-hint" class="mt-1 text-xs" style="color: var(--text-tertiary);">
							Bitte keine genaue Anschrift angeben.
						</p>
						{#if fieldErrors.location}
							<p id="location-error" class="mt-1 text-sm" style="color: var(--color-error);">
								{fieldErrors.location}
							</p>
						{/if}
					</div>
					<div class="sm:col-span-2">
						<p class="mb-3 text-sm" style="color: var(--text-tertiary);">
							E-Mail: <span data-testid="profile-email">{profile.email}</span>
						</p>
						<button
							type="submit"
							class="rounded-lg px-4 py-2 text-sm font-semibold disabled:opacity-50"
							style="background: var(--color-brand-500); color: #000;"
							disabled={profileSaving}
							data-testid="save-profile"
						>
							{profileSaving ? 'Speichern …' : 'Profildaten speichern'}
						</button>
					</div>
				</form>
			</section>

			<section
				class="glass-elevated mb-6 rounded-lg p-6"
				aria-labelledby="privacy-consents-heading"
			>
				<h2 id="privacy-consents-heading" class="mb-2 text-lg font-semibold">
					Datenschutz-Einwilligungen
				</h2>
				<p class="mb-4 text-sm" style="color: var(--text-secondary);">
					Diese Historie ist nur für dich sichtbar und wird bei späteren Versionen nicht
					überschrieben.
				</p>
				{#if privacyConsentsError}
					<p
						class="text-sm"
						role="alert"
						style="color: var(--color-error);"
						data-testid="privacy-consents-error"
					>
						{privacyConsentsError}
					</p>
				{:else if privacyConsents.length === 0}
					<p class="text-sm" data-testid="privacy-consents-empty">
						Für dieses Konto wurde noch kein versionierter Eintrag übernommen.
					</p>
				{:else}
					<ul class="space-y-3" data-testid="privacy-consents-list">
						{#each privacyConsents as consent (consent.policy_version)}
							<li class="rounded-lg p-3" style="background: var(--surface-raised);">
								<strong>Version {consent.policy_version}</strong>
								<span class="block text-sm" style="color: var(--text-secondary);">
									{new Date(consent.consented_at).toLocaleString('de-DE')} ·
									{registrationMethodLabel(consent.registration_method)}
								</span>
							</li>
						{/each}
					</ul>
				{/if}
			</section>

			<section class="glass-elevated rounded-lg p-6" aria-labelledby="visibility-heading">
				<h2 id="visibility-heading" class="mb-2 text-lg font-semibold">Sichtbarkeit</h2>
				<p class="mb-5 text-sm" style="color: var(--text-secondary);">
					Profil und Sammlung können unabhängig voneinander freigegeben werden.
				</p>

				<div class="space-y-5">
					<label class="flex cursor-pointer items-start justify-between gap-4" for="profile-public">
						<span>
							<span class="block font-medium">Profil öffentlich</span>
							<span class="block text-sm" style="color: var(--text-tertiary);">
								Anzeigename, Avatar, Standort und Mitgliedsdatum werden öffentlich sichtbar.
							</span>
						</span>
						<input
							id="profile-public"
							type="checkbox"
							bind:checked={profilePublic}
							disabled={saving}
							class="mt-1 h-5 w-5"
							data-testid="profile-public-toggle"
						/>
					</label>

					<label
						class="flex cursor-pointer items-start justify-between gap-4"
						for="collection-public"
					>
						<span>
							<span class="block font-medium">Sammlung öffentlich</span>
							<span class="block text-sm" style="color: var(--text-tertiary);">
								Andere können deine Sammlung und ihre Statistiken ansehen.
							</span>
							<strong
								id="collection-public-warning"
								class="mt-1 block text-sm"
								style="color: var(--color-warning);"
							>
								Öffentliche Sammlungen zeigen auch deine persönlichen Heftnotizen.
							</strong>
						</span>
						<input
							id="collection-public"
							type="checkbox"
							bind:checked={collectionPublic}
							disabled={saving}
							aria-describedby="collection-public-warning"
							class="mt-1 h-5 w-5"
							data-testid="collection-public-toggle"
						/>
					</label>
				</div>

				<div class="mt-6 flex flex-wrap items-center gap-3">
					<button
						type="button"
						class="rounded-lg px-4 py-2 text-sm font-semibold disabled:opacity-50"
						style="background: var(--color-brand-500); color: #000;"
						disabled={saving}
						onclick={saveVisibility}
						data-testid="save-visibility"
					>
						{saving ? 'Speichern …' : 'Sichtbarkeit speichern'}
					</button>
					{#if collectionPublic}
						<a href={resolve(`/users/${profile.id}/collection`)} class="text-sm underline">
							Öffentliche Sammlung ansehen
						</a>
					{/if}
				</div>
			</section>

			<section
				class="glass-elevated mt-6 rounded-lg border p-6"
				style="border-color: var(--color-error);"
				aria-labelledby="delete-account-heading"
			>
				<h2 id="delete-account-heading" class="text-lg font-semibold">Konto löschen</h2>
				<p class="mt-2 text-sm" style="color: var(--text-secondary);">
					Das Konto wird sofort deaktiviert und nach sieben Tagen endgültig gelöscht. Profil,
					Sammlung, Fotos und Zugangsdaten werden entfernt; gemeinsame abgeschlossene
					Tauschhistorien bleiben anonymisiert erhalten. Laufende Tausche werden sofort abgebrochen
					und bei einem Widerruf nicht wieder geöffnet.
				</p>
				{#if deletionOptionsError}
					<div class="mt-4" aria-live="polite">
						<p
							role="alert"
							style="color: var(--color-error);"
							data-testid="account-deletion-options-error"
						>
							{deletionOptionsError}
						</p>
						<button
							type="button"
							class="mt-2 rounded-lg border px-3 py-2 text-sm disabled:opacity-50"
							disabled={deletionOptionsLoading}
							onclick={reloadDeletionOptions}
							data-testid="retry-account-deletion-options"
						>
							{deletionOptionsLoading ? 'Wird geladen …' : 'Erneut versuchen'}
						</button>
					</div>
				{/if}
				<button
					type="button"
					class="mt-5 rounded-lg border px-4 py-2 font-semibold disabled:opacity-50"
					style="border-color: var(--color-error-foreground); color: var(--color-error-foreground);"
					disabled={!deletionOptions || deletionOffline}
					onclick={openDeletionDialog}
					data-testid="open-account-deletion"
				>
					Konto löschen …
				</button>
				{#if deletionOffline}
					<p class="mt-2 text-sm">Diese Aktion ist offline nicht verfügbar.</p>
				{/if}
			</section>
		{/if}

		<div class="mt-4 min-h-6" aria-live="polite">
			{#if error}
				<p role="alert" style="color: var(--color-error);" data-testid="profile-error">{error}</p>
			{:else if success}
				<p style="color: var(--color-success);" data-testid="profile-success">{success}</p>
			{/if}
		</div>
	</div>
</div>

<dialog
	bind:this={deletionDialog}
	class="glass-elevated m-auto w-[min(92vw,36rem)] rounded-xl p-6 backdrop:bg-black/60"
	onclose={() => (deletionError = null)}
>
	<h2 class="text-xl font-bold">Kontolöschung bestätigen</h2>
	{#if deletionOptions}
		<p class="mt-3 text-sm" style="color: var(--text-secondary);">
			Du kannst die Löschung innerhalb von {deletionOptions.grace_days} Tagen widerrufen.
		</p>

		{#if !deletionOptions.recent_authentication}
			<div class="mt-5">
				<p class="mb-2 font-medium">Anmeldung erneut bestätigen</p>
				{#if deletionOptions.password}
					<label for="deletion-password" class="mb-1 block text-sm">Passwort</label>
					<input
						id="deletion-password"
						type="password"
						autocomplete="current-password"
						bind:value={deletionPassword}
						class="w-full rounded-lg border px-3 py-2"
					/>
				{/if}
				{#if availableOAuthMethods(deletionOptions).length > 0}
					<div class="mt-3 flex flex-wrap gap-2">
						{#each availableOAuthMethods(deletionOptions) as provider (provider)}
							<button
								type="button"
								class="rounded-lg border px-3 py-2 text-sm"
								disabled={oauthReauthLoading !== null}
								onclick={() => reauthenticateOAuth(provider)}
							>
								{oauthReauthLoading === provider
									? 'Weiterleitung …'
									: `Mit ${provider === 'google' ? 'Google' : 'GitHub'} bestätigen`}
							</button>
						{/each}
					</div>
				{/if}
			</div>
		{/if}

		<form class="mt-5" onsubmit={deleteAccount}>
			<label for="deletion-confirmation" class="mb-1 block text-sm font-medium">
				Gib exakt <strong>{deletionOptions.confirmation_phrase}</strong> ein:
			</label>
			<input
				id="deletion-confirmation"
				type="text"
				bind:value={deletionConfirmation}
				autocomplete="off"
				class="w-full rounded-lg border px-3 py-2"
				data-testid="account-deletion-confirmation"
			/>
			<div class="mt-5 flex flex-wrap justify-end gap-3">
				<button type="button" class="rounded-lg px-4 py-2" onclick={() => deletionDialog?.close()}>
					Abbrechen
				</button>
				<button
					type="submit"
					class="rounded-lg px-4 py-2 font-semibold disabled:opacity-50"
					style="background: var(--color-error); color: white;"
					disabled={deleting ||
						deletionConfirmation !== deletionOptions.confirmation_phrase ||
						deletionOffline}
					data-testid="confirm-account-deletion"
				>
					{deleting
						? deletionScheduled
							? 'Lokale Daten werden gelöscht …'
							: 'Konto wird deaktiviert …'
						: deletionScheduled
							? 'Lokale Daten erneut löschen'
							: 'Konto endgültig vormerken'}
				</button>
			</div>
		</form>
		<div class="mt-3 min-h-6" aria-live="assertive">
			{#if deletionError}<p role="alert" style="color: var(--color-error);">{deletionError}</p>{/if}
		</div>
	{/if}
</dialog>
