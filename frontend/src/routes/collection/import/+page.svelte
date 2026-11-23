<script lang="ts">
	import { onDestroy } from 'svelte';
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { getAuthState } from '#lib/stores/auth.svelte.js';
	import { getOfflineStatus } from '#lib/offline/status.svelte.js';
	import {
		commitCollectionImport,
		downloadCollection,
		fetchCollectionImport,
		previewCollectionImport,
		refreshImportedCollection,
		rowStatusLabels,
		transferRowReason,
		transferStatusLabels,
		type TransferFormat,
		type TransferJob
	} from '#lib/api/collection-transfer.js';

	const auth = getAuthState();
	const offline = getOfflineStatus();
	let fileInput: HTMLInputElement;
	let file = $state<File | null>(null);
	let format = $state<TransferFormat>('csv');
	let job = $state<TransferJob | null>(null);
	let busy = $state(false);
	let error = $state<string | null>(null);
	let notice = $state<string | null>(null);
	let timer: ReturnType<typeof setTimeout> | undefined;
	let disposed = false;
	let requestGeneration = 0;
	let snapshotRefresh: Promise<boolean> | undefined;
	let restored = false;
	const offlineUnavailable = $derived(auth.isOfflineSession || !offline.online);
	const unavailable = $derived(auth.isLoading || !auth.isAuthenticated || offlineUnavailable);

	$effect(() => {
		if (!auth.isLoading && !auth.isAuthenticated) void goto(resolve('login'));
		else if (
			!auth.isLoading &&
			auth.isAuthenticated &&
			!restored &&
			typeof window !== 'undefined'
		) {
			restored = true;
			const id = new URLSearchParams(window.location.search).get('job');
			if (id) void restore(id);
		}
	});
	onDestroy(() => {
		disposed = true;
		clearTimeout(timer);
	});

	function beginJobRequest() {
		clearTimeout(timer);
		return ++requestGeneration;
	}
	function isCurrent(generation: number) {
		return !disposed && generation === requestGeneration;
	}
	async function acceptResult(result: TransferJob, generation: number) {
		if (!isCurrent(generation)) return;
		job = result;
		if (result.format === 'csv' || result.format === 'json') format = result.format;
		if (new URLSearchParams(window.location.search).get('job') !== result.id)
			void goto(resolve(`collection/import?job=${encodeURIComponent(result.id)}`), {
				replace: true,
				reset: false
			});
		if (result.status === 'completed') {
			const refreshed = await (snapshotRefresh ??= refreshImportedCollection());
			if (isCurrent(generation))
				notice = refreshed
					? 'Die Sammlung und der Offline-Bestand wurden aktualisiert.'
					: 'Der Import ist auf dem Server abgeschlossen. Der Offline-Bestand konnte noch nicht aktualisiert werden. Öffne die Sammlung erneut, sobald die Verbindung verfügbar ist.';
		} else if (['queued', 'running'].includes(result.status)) {
			timer = setTimeout(() => void reload(result.page), 1_500);
		}
	}
	async function restore(id: string) {
		const generation = beginJobRequest();
		busy = true;
		try {
			await acceptResult(await fetchCollectionImport(id), generation);
		} catch (cause) {
			if (isCurrent(generation))
				error = cause instanceof Error ? cause.message : 'Der Auftrag konnte nicht geladen werden.';
		} finally {
			if (!disposed) busy = false;
		}
	}
	async function reload(page = 1) {
		if (!job || disposed) return;
		const generation = beginJobRequest();
		error = null;
		try {
			await acceptResult(await fetchCollectionImport(job.id, page), generation);
		} catch (cause) {
			if (isCurrent(generation))
				error =
					cause instanceof Error
						? cause.message
						: 'Der Auftragsstatus konnte nicht geladen werden.';
		}
	}
	async function preview() {
		if (!file || busy || unavailable) return;
		const generation = beginJobRequest();
		busy = true;
		error = null;
		notice = null;
		job = null;
		snapshotRefresh = undefined;
		try {
			await acceptResult(await previewCollectionImport(file, format), generation);
		} catch (cause) {
			if (isCurrent(generation))
				error = cause instanceof Error ? cause.message : 'Die Datei konnte nicht geprüft werden.';
		} finally {
			if (!disposed) busy = false;
		}
	}
	async function confirm() {
		if (!job || busy || unavailable) return;
		const generation = beginJobRequest();
		busy = true;
		error = null;
		try {
			await acceptResult(await commitCollectionImport(job.id), generation);
		} catch (cause) {
			if (isCurrent(generation))
				error =
					cause instanceof Error ? cause.message : 'Der Import konnte nicht gestartet werden.';
		} finally {
			if (!disposed) busy = false;
		}
	}
	async function download(selected: TransferFormat, template = false) {
		busy = true;
		error = null;
		try {
			await downloadCollection(selected, template);
		} catch (cause) {
			if (!disposed)
				error = cause instanceof Error ? cause.message : 'Der Download ist fehlgeschlagen.';
		} finally {
			if (!disposed) busy = false;
		}
	}
	function clearPreview() {
		beginJobRequest();
		job = null;
		notice = null;
		error = null;
		snapshotRefresh = undefined;
	}
</script>

<svelte:head><title>Sammlung importieren und exportieren – LILLY</title></svelte:head>
<div class="mx-auto max-w-5xl space-y-6 px-4 py-8" style="color: var(--text-primary);">
	<a href={resolve('collection')} class="text-sm underline">Zur Sammlung</a>
	<h1 class="text-2xl font-bold">Sammlung importieren und exportieren</h1>
	{#if offlineUnavailable}<p role="status" class="glass-elevated rounded-xl p-4">
			Import und Export benötigen eine Internetverbindung.
		</p>{/if}
	{#if error}<p role="alert" class="glass-elevated rounded-xl p-4" data-testid="transfer-error">
			{error}
		</p>{/if}
	{#if notice}<p role="status" class="glass-elevated rounded-xl p-4">{notice}</p>{/if}
	<section
		class="glass-elevated space-y-4 rounded-xl p-6"
		aria-labelledby="collection-export-heading"
	>
		<h2 id="collection-export-heading" class="text-lg font-semibold">
			Eigene Sammlung exportieren
		</h2>
		<p>
			Der Download enthält deine gesamte Sammlung einschließlich Editionen, Zustand und Notizen.
			Aktuelle Suchfilter begrenzen den Export nicht.
		</p>
		<div class="flex flex-wrap gap-3">
			<button
				type="button"
				class="btn variant-soft"
				disabled={busy || unavailable}
				onclick={() => download('csv')}
				data-testid="collection-export-csv"
			>
				CSV herunterladen
			</button>
			<button
				type="button"
				class="btn variant-soft"
				disabled={busy || unavailable}
				onclick={() => download('json')}
				data-testid="collection-export-json"
			>
				JSON herunterladen
			</button>
		</div>
	</section>
	<section
		class="glass-elevated space-y-4 rounded-xl p-6"
		aria-labelledby="collection-import-heading"
	>
		<h2 id="collection-import-heading" class="text-lg font-semibold">Bestandsliste übernehmen</h2>
		<p>
			Verwende eine LILLY-Vorlage. Jedes Exemplar braucht eine dauerhaft eindeutige Kennung. Neue
			Einträge werden ergänzt; vorhandene Angaben bleiben erhalten. Die Vorschau gilt 30 Minuten.
		</p>
		<p>
			Voreinstellung: bis zu 10 MiB und 10.000 Datensätze je Datei. Abweichende Servergrenzen werden
			bei der Prüfung gemeldet.
		</p>
		<div class="flex flex-wrap gap-3">
			<button
				type="button"
				class="btn variant-soft"
				disabled={busy || unavailable}
				onclick={() => download('csv', true)}
			>
				CSV-Vorlage
			</button>
			<button
				type="button"
				class="btn variant-soft"
				disabled={busy || unavailable}
				onclick={() => download('json', true)}
			>
				JSON-Vorlage
			</button>
		</div>
		<form
			class="space-y-4"
			onsubmit={(event) => {
				event.preventDefault();
				void preview();
			}}
		>
			<div>
				<label for="import-format" class="mb-1 block">Dateiformat</label><select
					id="import-format"
					bind:value={format}
					onchange={() => {
						fileInput.value = '';
						file = null;
						clearPreview();
					}}
					class="select"
					disabled={busy || unavailable}
				>
					<option value="csv">LILLY CSV</option><option value="json">LILLY JSON</option>
				</select>
			</div>
			<div>
				<label for="import-file" class="mb-1 block">Bestandsdatei</label><input
					id="import-file"
					type="file"
					bind:this={fileInput}
					accept={format === 'csv' ? '.csv,text/csv' : '.json,application/json'}
					disabled={busy || unavailable}
					onchange={(event) => {
						clearPreview();
						file = event.currentTarget.files?.[0] ?? null;
					}}
					data-testid="collection-import-file"
				/>
			</div>
			<button
				type="submit"
				class="btn variant-filled"
				disabled={!file || busy || unavailable}
				data-testid="collection-import-preview"
			>
				{busy ? 'Bitte warten …' : 'Datei prüfen'}
			</button>
		</form>
	</section>
	{#if job}
		<section
			class="glass-elevated space-y-4 rounded-xl p-6"
			aria-labelledby="import-result-heading"
			aria-busy={job.status === 'running' || job.status === 'queued'}
			data-testid="import-result"
		>
			<h2 id="import-result-heading" class="text-lg font-semibold">
				{transferStatusLabels[job.status]}
			</h2>
			<p role="status" aria-live="polite" data-testid="import-counts">
				{job.total_rows} Datensätze: {job.counts.new} neu, {job.counts.created} hinzugefügt, {job
					.counts.unchanged} übersprungen, {job.counts.conflict} Konflikte, {job.counts.invalid} ungültig,
				{job.counts.failed} fehlgeschlagen.
			</p>
			{#if job.status === 'prepared'}
				<p>
					Es wurden noch keine Sammlungseinträge verändert. Konflikte und ungültige Zeilen werden
					ausgelassen.
				</p>
				<button
					type="button"
					class="btn variant-filled"
					disabled={busy || unavailable || job.counts.new === 0}
					onclick={confirm}
					data-testid="collection-import-confirm"
				>
					Gültige neue Exemplare importieren
				</button>
			{:else if job.status === 'failed'}
				<p role="alert">
					Der Auftrag wurde unterbrochen. Bereits hinzugefügte Einträge bleiben erhalten. Du kannst
					dieselbe Datei erneut prüfen; vorhandene Exemplare werden erkannt.
				</p>
			{:else if job.status === 'expired' || job.status === 'cancelled'}
				<p>Bitte prüfe die Datei erneut, um eine neue Vorschau zu erhalten.</p>
			{/if}
			<button
				type="button"
				class="btn variant-soft"
				disabled={busy || unavailable}
				onclick={() => reload(job?.page)}
			>
				Status aktualisieren
			</button>
			<!-- svelte-ignore a11y_no_noninteractive_tabindex (Keyboard users need to scroll wide results.) -->
			<div class="overflow-x-auto" role="region" aria-label="Import-Ergebnistabelle" tabindex="0">
				<table class="w-full text-left text-sm">
					<caption class="sr-only">Ergebnis je Datensatz</caption><thead>
						<tr>
							<th scope="col" class="p-2">Datensatz</th><th scope="col" class="p-2">Kennung</th><th
								scope="col"
								class="p-2"
							>
								Ergebnis
							</th><th scope="col" class="p-2">Hinweis</th>
						</tr>
					</thead><tbody>
						{#each job.results as row (row.row_number)}<tr>
								<th scope="row" class="p-2">{row.row_number}</th><td class="break-all p-2">
									{row.copy_key ?? '–'}
								</td><td class="p-2">{rowStatusLabels[row.status]}</td><td class="p-2">
									{transferRowReason(row.code)}
								</td>
							</tr>{/each}
					</tbody>
				</table>
			</div>
			<nav aria-label="Import-Ergebnisse" class="flex items-center gap-3">
				<button
					type="button"
					class="btn variant-soft"
					disabled={job.page <= 1 || busy || unavailable}
					onclick={() => reload((job?.page ?? 1) - 1)}
				>
					Zurück
				</button>
				<span>Seite {job.page}</span>
				<button
					type="button"
					class="btn variant-soft"
					disabled={job.page * job.per_page >= job.total_rows || busy || unavailable}
					onclick={() => reload((job?.page ?? 1) + 1)}
				>
					Weiter
				</button>
			</nav>
		</section>
	{/if}
</div>
