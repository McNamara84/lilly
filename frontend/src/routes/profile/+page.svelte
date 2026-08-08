<script lang="ts">
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { fetchOwnProfile, updateVisibility, type OwnProfile } from '$lib/api/profile';
	import { getAuthState } from '$lib/stores/auth.svelte';

	const auth = getAuthState();

	let profile = $state<OwnProfile | null>(null);
	let profilePublic = $state(false);
	let collectionPublic = $state(false);
	let loading = $state(true);
	let saving = $state(false);
	let error = $state<string | null>(null);
	let success = $state<string | null>(null);

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
		try {
			profile = await fetchOwnProfile();
			profilePublic = profile.profile_public;
			collectionPublic = profile.collection_public;
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'Profil konnte nicht geladen werden.';
		} finally {
			loading = false;
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
				<h2 id="profile-data-heading" class="mb-4 text-lg font-semibold">Kontodaten</h2>
				<dl class="grid gap-3 sm:grid-cols-2">
					<div>
						<dt class="text-xs" style="color: var(--text-tertiary);">Anzeigename</dt>
						<dd data-testid="profile-display-name">{profile.display_name}</dd>
					</div>
					<div>
						<dt class="text-xs" style="color: var(--text-tertiary);">E-Mail</dt>
						<dd data-testid="profile-email">{profile.email}</dd>
					</div>
				</dl>
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
