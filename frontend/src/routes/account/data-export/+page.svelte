<script lang="ts">
	import { onDestroy } from 'svelte';
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { getAuthState } from '#lib/stores/auth.svelte.js';
	import { getOfflineStatus } from '#lib/offline/status.svelte.js';
	import { startOAuth, type OAuthProvider } from '#lib/api/auth.js';
	import { reauthenticateWithPassword } from '#lib/api/account-erasure.js';
	import {
		downloadPersonalExport,
		fetchReauthenticationOptions,
		listPersonalExports,
		requestPersonalExport,
		TransferError,
		transferStatusLabels,
		type TransferJob,
		type ReauthenticationOptions
	} from '#lib/api/collection-transfer.js';

	const auth = getAuthState();
	const offline = getOfflineStatus();
	let options = $state<ReauthenticationOptions | null>(null);
	let jobs = $state<TransferJob[]>([]);
	let password = $state('');
	let busy = $state(false);
	let loading = $state(true);
	let initialized = false;
	let disposed = false;
	let error = $state<string | null>(null);
	let notice = $state<string | null>(null);
	let timer: ReturnType<typeof setTimeout> | undefined;
	let pageNumber = $state(1);
	let loadVersion = 0;
	const unavailable = $derived(auth.isOfflineSession || !offline.online);
	const running = $derived(jobs.some((job) => job.status === 'queued' || job.status === 'running'));
	const date = (value: string) =>
		new Intl.DateTimeFormat('de-DE', { dateStyle: 'medium', timeStyle: 'short' }).format(
			new Date(value)
		);

	$effect(() => {
		if (!auth.isLoading && !auth.isAuthenticated) void goto(resolve('login'));
		else if (!auth.isLoading && auth.isAuthenticated && !initialized) {
			initialized = true;
			void reload();
		}
	});
	onDestroy(() => {
		disposed = true;
		clearTimeout(timer);
		password = '';
	});

	async function reload(requestedPage = pageNumber) {
		const version = ++loadVersion;
		clearTimeout(timer);
		error = null;
		try {
			const [nextOptions, nextJobs] = await Promise.all([
				fetchReauthenticationOptions(),
				listPersonalExports(requestedPage)
			]);
			if (disposed || version !== loadVersion) return;
			pageNumber = requestedPage;
			options = nextOptions;
			jobs = nextJobs;
			if (nextJobs.some((job) => ['queued', 'running'].includes(job.status)))
				timer = setTimeout(() => void reload(requestedPage), 1_500);
		} catch (cause) {
			if (!disposed && version === loadVersion)
				error =
					cause instanceof Error
						? cause.message
						: 'Die Exportaufträge konnten nicht geladen werden.';
		} finally {
			if (!disposed && version === loadVersion) loading = false;
		}
	}
	async function reauthenticate() {
		busy = true;
		error = null;
		notice = null;
		try {
			await reauthenticateWithPassword(password);
			if (!disposed) {
				await reload();
				notice = 'Deine Anmeldung ist bestätigt.';
			}
		} catch (cause) {
			if (!disposed)
				error = cause instanceof Error ? cause.message : 'Die Bestätigung ist fehlgeschlagen.';
		} finally {
			password = '';
			if (!disposed) busy = false;
		}
	}
	async function oauth(provider: OAuthProvider) {
		busy = true;
		error = null;
		try {
			const url = await startOAuth(provider, 'reauth');
			if (disposed) return;
			sessionStorage.setItem('lilly-data-export-reauth', 'pending');
			window.location.assign(url);
		} catch (cause) {
			if (!disposed) {
				error =
					cause instanceof Error ? cause.message : 'Die Anmeldung konnte nicht bestätigt werden.';
				busy = false;
			}
		}
	}
	async function createExport() {
		busy = true;
		error = null;
		notice = null;
		try {
			const job = await requestPersonalExport();
			if (!disposed) {
				jobs = [job, ...jobs.filter((existing) => existing.id !== job.id)];
				notice = 'Dein Export wird erstellt. Du kannst später zu dieser Seite zurückkehren.';
				await reload(1);
			}
		} catch (cause) {
			if (!disposed) {
				if (cause instanceof TransferError && cause.code === 'RECENT_AUTH_REQUIRED' && options)
					options = { ...options, recent_authentication: false };
				error =
					cause instanceof Error ? cause.message : 'Der Export konnte nicht angefordert werden.';
			}
		} finally {
			if (!disposed) busy = false;
		}
	}
	async function download(job: TransferJob) {
		busy = true;
		error = null;
		try {
			await downloadPersonalExport(job.id);
		} catch (cause) {
			if (!disposed) {
				if (cause instanceof TransferError && cause.code === 'RECENT_AUTH_REQUIRED' && options)
					options = { ...options, recent_authentication: false };
				error =
					cause instanceof Error ? cause.message : 'Der Download konnte nicht gestartet werden.';
			}
		} finally {
			if (!disposed) busy = false;
		}
	}
</script>

<svelte:head><title>Persönliche Daten exportieren – LILLY</title></svelte:head>
<div class="mx-auto max-w-3xl space-y-6 px-4 py-8" style="color: var(--text-primary);">
	<a href={resolve('profile')} class="text-sm underline">Zum Profil</a>
	<h1 class="text-2xl font-bold">Persönliche Daten exportieren</h1>
	<p>
		Du erhältst eine Kopie deiner gespeicherten Kontodaten, Sammlung, Einwilligungen, Tauschvorgänge
		und sichtbaren Nachrichten einschließlich empfangener Texte. Deine gespeicherten Fotos und dein
		Avatar sind enthalten.
	</p>
	<p>
		Das ZIP wird im Hintergrund erstellt und steht ab Fertigstellung 24 Stunden zum geschützten
		Download bereit. Danach wird es automatisch gelöscht.
	</p>
	<p>
		Die Auskunft enthält den gespeicherten Serverstand. Synchronisiere lokale Sammlungsänderungen
		vor der Anforderung.
	</p>
	{#if unavailable}<p role="status" class="glass-elevated rounded-xl p-4">
			Die Datenauskunft benötigt eine Internetverbindung.
		</p>{/if}
	{#if error}<p role="alert" class="glass-elevated rounded-xl p-4" data-testid="data-export-error">
			{error}
		</p>{/if}
	{#if notice}<p role="status" class="glass-elevated rounded-xl p-4">{notice}</p>{/if}
	{#if options && !options.recent_authentication}
		<section
			class="glass-elevated space-y-4 rounded-xl p-6"
			aria-labelledby="export-reauth-heading"
			data-testid="export-reauth"
		>
			<h2 id="export-reauth-heading" class="text-lg font-semibold">Anmeldung bestätigen</h2>
			<p>Für Anforderung und Download benötigst du eine höchstens zehn Minuten alte Anmeldung.</p>
			{#if options.password}
				<form
					class="space-y-3"
					onsubmit={(event) => {
						event.preventDefault();
						void reauthenticate();
					}}
				>
					<label for="export-password" class="block">Aktuelles Passwort</label>
					<input
						id="export-password"
						class="input w-full"
						type="password"
						autocomplete="current-password"
						bind:value={password}
						disabled={busy || unavailable}
						required
					/>
					<button
						type="submit"
						class="btn variant-filled"
						disabled={busy || unavailable || !password}
						data-testid="export-confirm-password"
					>
						Anmeldung bestätigen
					</button>
				</form>
			{/if}
			{#if options.google || options.github}
				<div class="flex flex-wrap gap-3">
					{#if options.google}<button
							type="button"
							class="btn variant-soft"
							disabled={busy || unavailable}
							onclick={() => oauth('google')}
						>
							Mit Google bestätigen
						</button>{/if}
					{#if options.github}<button
							type="button"
							class="btn variant-soft"
							disabled={busy || unavailable}
							onclick={() => oauth('github')}
						>
							Mit GitHub bestätigen
						</button>{/if}
				</div>
			{/if}
		</section>
	{/if}
	<div class="flex flex-wrap gap-3">
		<button
			type="button"
			class="btn variant-filled"
			disabled={loading || busy || unavailable || running || !options?.recent_authentication}
			onclick={createExport}
			data-testid="request-personal-export"
		>
			{running ? 'Export wird erstellt …' : 'Vollständigen Export anfordern'}
		</button>
		<button
			type="button"
			class="btn variant-soft"
			disabled={busy || unavailable}
			onclick={() => reload()}
		>
			Aktualisieren
		</button>
	</div>
	<section aria-labelledby="export-jobs-heading" class="space-y-4" aria-busy={loading}>
		<h2 id="export-jobs-heading" class="text-lg font-semibold">Deine Exportaufträge</h2>
		{#if loading}<p role="status">Aufträge werden geladen …</p>
		{:else if jobs.length === 0}<p>Noch kein Export angefordert.</p>
		{:else}
			<ul class="space-y-4">
				{#each jobs as job (job.id)}
					<li class="glass-elevated space-y-3 rounded-xl p-5" data-testid="personal-export-job">
						<p class="font-semibold" role="status" aria-live="polite">
							{transferStatusLabels[job.status]}
						</p>
						<p class="text-sm">Angefordert am {date(job.created_at)}</p>
						{#if job.status === 'ready'}
							<p>Download verfügbar bis {date(job.expires_at)}.</p>
							<button
								type="button"
								class="btn variant-filled"
								disabled={busy || unavailable}
								onclick={() => download(job)}
								data-testid="download-personal-export"
							>
								ZIP herunterladen
							</button>
						{:else if job.status === 'failed'}
							<p>
								Der Export konnte nicht vollständig erstellt werden. Bitte fordere ihn erneut an.
								Bei wiederholtem Fehlschlag wende dich an den Betreiber.
							</p>
						{:else if job.status === 'expired'}<p>
								Die Downloadfrist ist abgelaufen. Du kannst einen neuen Export anfordern.
							</p>
						{:else if job.status === 'cancelled'}<p>
								Dieser Export ist durch eine Änderung oder Löschung von Kontodaten nicht mehr
								gültig. Du kannst einen neuen Export anfordern.
							</p>
						{/if}
					</li>
				{/each}
			</ul>
		{/if}
	</section>
	<nav aria-label="Exportaufträge" class="flex items-center gap-3">
		<button
			type="button"
			class="btn variant-soft"
			disabled={loading || busy || unavailable || pageNumber <= 1}
			onclick={() => reload(pageNumber - 1)}
		>
			Neuere Aufträge
		</button>
		<span>Seite {pageNumber}</span>
		<button
			type="button"
			class="btn variant-soft"
			disabled={loading || busy || unavailable || jobs.length < 20}
			onclick={() => reload(pageNumber + 1)}
		>
			Ältere Aufträge
		</button>
	</nav>
</div>
