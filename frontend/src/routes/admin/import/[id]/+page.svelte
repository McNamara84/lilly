<script lang="ts">
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { page } from '$app/stores';
	import {
		activateImport,
		cancelImport,
		fetchImportErrors,
		fetchImportJob,
		fetchImportReviewItems,
		fetchImportReviewSummary,
		retryImport,
		type CoverStatus,
		type ImportJob,
		type ImportJobError,
		type ImportReviewSummary,
		type ReviewItem,
		type ReviewOutcome,
		type ReviewSeverity
	} from '$lib/api/admin';

	let job = $state<ImportJob | null>(null);
	let reviewSummary = $state<ImportReviewSummary | null>(null);
	let reviewItems = $state<ReviewItem[]>([]);
	let reviewTotal = $state(0);
	let reviewPage = $state(1);
	let query = $state('');
	let outcomeFilter = $state<ReviewOutcome | ''>('');
	let severityFilter = $state<ReviewSeverity | ''>('');
	let coverFilter = $state<CoverStatus | ''>('');
	let sampleOnly = $state(false);
	let acknowledgeWarnings = $state(false);
	let jobErrors = $state<ImportJobError[]>([]);
	let jobErrorsTotal = $state(0);
	let jobErrorsPage = $state(1);
	let loading = $state(true);
	let reviewLoading = $state(false);
	let error = $state<string | null>(null);
	let actionPending = $state(false);
	let polling = false;
	let pollTimeout: ReturnType<typeof setTimeout> | null = null;

	const jobId = $derived(Number($page.params.id));
	const invalidJobId = $derived(!Number.isFinite(jobId) || jobId < 1);

	function isTerminal(status: ImportJob['status']): boolean {
		return !['pending', 'running'].includes(status);
	}

	function canRetry(status: ImportJob['status']): boolean {
		return ['failed', 'cancelled', 'interrupted'].includes(status);
	}

	function formatDate(date: string | undefined | null): string {
		return date ? new Date(date).toLocaleString('de-DE') : '–';
	}

	function processedCount(): number {
		if (!job) return 0;
		return job.imported_issues + (job.skipped_issues ?? 0) + job.failed_issues;
	}

	function progressPercent(): number {
		if (!job || job.total_issues === 0) return 0;
		return Math.round((processedCount() / job.total_issues) * 100);
	}

	async function loadJob() {
		if (invalidJobId) {
			error = 'Invalid import job ID';
			loading = false;
			return;
		}
		try {
			job = await fetchImportJob(jobId);
			if (isTerminal(job.status)) {
				stopPolling();
				const tasks: Promise<void>[] = [loadErrors()];
				if (job.status === 'completed' || job.status === 'completed_with_errors') {
					tasks.push(loadReview(true));
				}
				await Promise.all(tasks);
			}
		} catch (caught) {
			error = caught instanceof Error ? caught.message : 'Failed to load import job';
			stopPolling();
		} finally {
			loading = false;
		}
	}

	async function loadErrors() {
		try {
			const result = await fetchImportErrors(jobId, jobErrorsPage);
			jobErrors = result.data;
			jobErrorsTotal = result.total;
		} catch (caught) {
			error = caught instanceof Error ? caught.message : 'Failed to load import errors';
		}
	}

	async function loadReview(includeSummary = false) {
		reviewLoading = true;
		try {
			const [items, summary] = await Promise.all([
				fetchImportReviewItems(jobId, {
					page: reviewPage,
					query,
					outcome: outcomeFilter,
					severity: severityFilter,
					coverStatus: coverFilter,
					sample: sampleOnly
				}),
				includeSummary || reviewSummary === null
					? fetchImportReviewSummary(jobId)
					: Promise.resolve(reviewSummary)
			]);
			reviewItems = items.items;
			reviewTotal = items.total;
			reviewSummary = summary;
		} catch (caught) {
			error = caught instanceof Error ? caught.message : 'Failed to load review results';
		} finally {
			reviewLoading = false;
		}
	}

	async function applyReviewFilters() {
		reviewPage = 1;
		await loadReview();
	}

	async function changeReviewPage(nextPage: number) {
		reviewPage = nextPage;
		await loadReview();
	}

	async function changeErrorsPage(nextPage: number) {
		jobErrorsPage = nextPage;
		await loadErrors();
	}

	async function handleActivate() {
		if (!job || !reviewSummary) return;
		actionPending = true;
		error = null;
		try {
			const response = await activateImport(job.id, acknowledgeWarnings);
			if (response.active) {
				reviewSummary = { ...reviewSummary, series_active: true };
			}
		} catch (caught) {
			error = caught instanceof Error ? caught.message : 'Activation failed';
		} finally {
			actionPending = false;
		}
	}

	async function handleCancel() {
		if (!job) return;
		actionPending = true;
		error = null;
		try {
			job = await cancelImport(job.id);
		} catch (caught) {
			error = caught instanceof Error ? caught.message : 'Cancellation failed';
		} finally {
			actionPending = false;
		}
	}

	async function handleRetry() {
		if (!job) return;
		actionPending = true;
		error = null;
		try {
			const retry = await retryImport(job.id);
			await goto(resolve(`/admin/import/${retry.id}`));
		} catch (caught) {
			error = caught instanceof Error ? caught.message : 'Retry failed';
		} finally {
			actionPending = false;
		}
	}

	function startPolling() {
		polling = true;
		scheduleNextPoll();
	}

	function scheduleNextPoll() {
		if (!polling) return;
		pollTimeout = setTimeout(async () => {
			await loadJob();
			scheduleNextPoll();
		}, 3000);
	}

	function stopPolling() {
		polling = false;
		if (pollTimeout) clearTimeout(pollTimeout);
		pollTimeout = null;
	}

	function resetPageState() {
		stopPolling();
		job = null;
		reviewSummary = null;
		reviewItems = [];
		reviewTotal = 0;
		reviewPage = 1;
		query = '';
		outcomeFilter = '';
		severityFilter = '';
		coverFilter = '';
		sampleOnly = false;
		acknowledgeWarnings = false;
		jobErrors = [];
		jobErrorsTotal = 0;
		jobErrorsPage = 1;
		loading = true;
		error = null;
		actionPending = false;
	}

	function coverLabel(status: CoverStatus): string {
		return {
			imported: 'Importiert',
			reused: 'Vorhanden',
			missing_at_source: 'Nicht in Quelle',
			not_permitted: 'Nicht erlaubt',
			fetch_failed: 'Abruf fehlgeschlagen',
			invalid: 'Ungültig',
			storage_failed: 'Speichern fehlgeschlagen',
			not_checked: 'Nicht geprüft'
		}[status];
	}

	$effect(() => {
		const currentJobId = jobId;
		resetPageState();
		loadJob();
		if (!Number.isFinite(currentJobId) || currentJobId < 1) return;
		startPolling();
		return () => stopPolling();
	});
</script>

<svelte:head>
	<title>Import #{jobId} – LILLY Admin</title>
</svelte:head>

<a
	href={resolve('/admin/import')}
	class="text-sm mb-4 inline-block"
	style="color: var(--color-brand-500);"
	data-testid="back-link">&larr; Zurück zur Import-Übersicht</a
>

{#if loading}
	<p style="color: var(--text-secondary);" data-testid="loading-indicator">
		Lade Import-Details...
	</p>
{:else if error}
	<div
		class="p-4 rounded-lg mb-4"
		style="background-color: var(--color-error-100); color: var(--color-error-700);"
		role="alert"
		data-testid="error-message"
	>
		{error}
	</div>
{:else if job}
	<h1
		class="text-2xl font-bold mb-2"
		style="color: var(--text-primary);"
		data-testid="import-title"
	>
		Import #{job.id}
	</h1>
	<p class="text-sm mb-6" style="color: var(--text-secondary);">
		Adapter: {job.adapter_name}
		{#if job.source_key}· Quelle: {job.source_key}{/if}
		· {job.trigger_type === 'scheduled' ? 'Automatischer Lauf' : 'Manueller Lauf'}
		{#if job.retry_of_job_id}· Wiederholung von #{job.retry_of_job_id}{/if}
		· Zuletzt aktualisiert: {formatDate(job.updated_at)}
	</p>

	<section class="glass-elevated p-6 rounded-lg mb-6" data-testid="progress-section">
		<div class="flex items-center justify-between mb-4 gap-4">
			<span class="text-sm font-medium" style="color: var(--text-primary);">
				Status: <span data-testid="job-status">{job.status}</span>
			</span>
			<span class="text-sm" style="color: var(--text-secondary);" data-testid="progress-count">
				{processedCount()} / {job.total_issues} bearbeitet ({job.created_issues ?? 0} neu,
				{job.updated_issues ?? 0} geändert, {job.unchanged_issues ?? 0} unverändert,
				{job.skipped_issues ?? 0} übersprungen, {job.failed_issues} fehlgeschlagen)
			</span>
		</div>
		<div
			class="w-full h-3 rounded-full overflow-hidden"
			style="background-color: var(--surface-base);"
			role="progressbar"
			aria-valuenow={processedCount()}
			aria-valuemin={0}
			aria-valuemax={job.total_issues}
			aria-label="Import-Fortschritt"
			data-testid="progress-bar"
		>
			<div
				class="h-full rounded-full transition-all duration-300"
				style="width: {progressPercent()}%; background-color: var(--color-brand-500);"
			></div>
		</div>
		{#if job.error_message}
			<p class="mt-3 text-sm" style="color: var(--color-error-500);" data-testid="error-detail">
				Fehler: {job.error_message}
			</p>
		{/if}
		<div class="mt-4 flex gap-3">
			{#if job.status === 'pending' || job.status === 'running'}
				<button
					onclick={handleCancel}
					disabled={actionPending || job.cancel_requested_at != null}
					class="px-4 py-2 rounded-lg text-sm font-medium disabled:opacity-50"
					style="background-color: var(--color-error-500); color: white;"
					data-testid="cancel-import-button"
					>{job.cancel_requested_at ? 'Abbruch angefordert' : 'Import abbrechen'}</button
				>
			{:else if canRetry(job.status)}
				<button
					onclick={handleRetry}
					disabled={actionPending}
					class="px-4 py-2 rounded-lg text-sm font-medium disabled:opacity-50"
					style="background-color: var(--color-brand-500); color: white;"
					data-testid="retry-import-button">Erneut vollständig synchronisieren</button
				>
			{/if}
		</div>
	</section>

	{#if jobErrors.length > 0}
		<section class="glass-elevated p-6 rounded-lg mb-6" data-testid="job-errors-section">
			<h2 class="text-lg font-semibold mb-3" style="color: var(--text-primary);">Fehlerkontext</h2>
			<ul class="space-y-2 text-sm">
				{#each jobErrors as item (item.id)}
					<li style="color: var(--color-error-700);">
						{item.issue_number === null ? 'Lauf' : `Heft #${item.issue_number}`}
						({item.source_key}{item.source_record_id ? `:${item.source_record_id}` : ''}) [{item.stage}]:
						{item.message} ({item.severity ?? 'blocking'})
					</li>
				{/each}
			</ul>
			{#if jobErrorsTotal > 50}
				<div class="mt-4 flex items-center gap-3 text-sm">
					<button
						onclick={() => changeErrorsPage(jobErrorsPage - 1)}
						disabled={jobErrorsPage <= 1}
						data-testid="previous-errors-page">Zurück</button
					>
					<span>Seite {jobErrorsPage}</span>
					<button
						onclick={() => changeErrorsPage(jobErrorsPage + 1)}
						disabled={jobErrorsPage * 50 >= jobErrorsTotal}
						data-testid="next-errors-page">Weiter</button
					>
				</div>
			{/if}
		</section>
	{/if}

	{#if (job.status === 'completed' || job.status === 'completed_with_errors') && reviewSummary}
		<section class="mb-6" data-testid="review-section">
			<div class="glass-elevated p-5 rounded-lg mb-5" data-testid="review-summary">
				<div class="flex flex-wrap items-center justify-between gap-3">
					<h2 class="text-lg font-semibold" style="color: var(--text-primary);">
						Prüfung: {reviewSummary.series_name}
					</h2>
					<span class="text-sm" style="color: var(--text-secondary);">
						{reviewSummary.warning_count} Warnungen · {reviewSummary.blocking_count} Blocker
					</span>
				</div>
				<p class="mt-2 text-sm" style="color: var(--text-secondary);">
					{reviewSummary.outcomes.created} neu, {reviewSummary.outcomes.updated} geändert,
					{reviewSummary.outcomes.unchanged} unverändert, {reviewSummary.outcomes.skipped}
					übersprungen, {reviewSummary.outcomes.failed} fehlgeschlagen
				</p>

				{#if reviewSummary.eligibility.reasons.length > 0}
					<ul
						class="mt-4 p-3 rounded text-sm list-disc pl-8"
						style="background-color: var(--color-error-100); color: var(--color-error-700);"
						data-testid="activation-blockers"
					>
						{#each reviewSummary.eligibility.reasons as reason (reason.code)}
							<li>{reason.message} ({reason.code})</li>
						{/each}
					</ul>
				{/if}

				{#if reviewSummary.eligibility.requires_acknowledgement}
					<label
						class="mt-4 flex items-start gap-2 p-3 rounded text-sm"
						style="background-color: var(--color-warning-100); color: var(--text-primary);"
						data-testid="warning-acknowledgement"
					>
						<input type="checkbox" bind:checked={acknowledgeWarnings} />
						<span>Ich habe alle Warnungen geprüft und gebe die Serie trotzdem frei.</span>
					</label>
				{/if}

				<div class="mt-4">
					{#if reviewSummary.series_active}
						<span style="color: var(--color-success-700);" data-testid="series-active-message">
							Serie ist veröffentlicht.
						</span>
					{:else}
						<button
							onclick={handleActivate}
							disabled={!reviewSummary.eligibility.eligible ||
								actionPending ||
								(reviewSummary.eligibility.requires_acknowledgement && !acknowledgeWarnings)}
							class="px-4 py-2 rounded-lg text-sm font-medium disabled:opacity-50"
							style="background-color: var(--color-success-500); color: white;"
							data-testid="activate-series-button">Geprüften Import freigeben</button
						>
					{/if}
				</div>
			</div>

			<div class="glass-elevated p-5 rounded-lg mb-5" data-testid="reference-checks">
				<h3 class="font-semibold mb-3">Referenzprüfungen</h3>
				<ul class="grid gap-2 text-sm md:grid-cols-2">
					{#each reviewSummary.reference_checks as check (check.issue_number)}
						<li>
							<strong>#{check.issue_number} {check.expected_title}</strong> –
							<span
								style:color={check.status === 'passed'
									? 'var(--color-success-700)'
									: 'var(--color-error-700)'}
							>
								{check.status === 'passed' ? 'bestanden' : 'fehlgeschlagen'}
							</span>
						</li>
					{/each}
				</ul>
			</div>

			<form
				class="glass-elevated p-4 rounded-lg mb-4 grid gap-3 md:grid-cols-6"
				onsubmit={(event) => {
					event.preventDefault();
					applyReviewFilters();
				}}
				data-testid="review-filters"
			>
				<label class="md:col-span-2 text-sm">
					<span class="block mb-1">Suche</span>
					<input
						class="w-full p-2 rounded"
						bind:value={query}
						placeholder="Nr., Titel, Autor, Quellen-ID"
					/>
				</label>
				<label class="text-sm">
					<span class="block mb-1">Ergebnis</span>
					<select class="w-full p-2 rounded" bind:value={outcomeFilter}>
						<option value="">Alle</option><option value="created">Neu</option><option
							value="updated">Geändert</option
						><option value="unchanged">Unverändert</option><option value="skipped"
							>Übersprungen</option
						><option value="failed">Fehlgeschlagen</option>
					</select>
				</label>
				<label class="text-sm">
					<span class="block mb-1">Risiko</span>
					<select class="w-full p-2 rounded" bind:value={severityFilter}>
						<option value="">Alle</option><option value="info">Info</option><option value="warning"
							>Warnung</option
						><option value="blocking">Blocker</option>
					</select>
				</label>
				<label class="text-sm">
					<span class="block mb-1">Cover</span>
					<select class="w-full p-2 rounded" bind:value={coverFilter}>
						<option value="">Alle</option><option value="imported">Importiert</option><option
							value="reused">Vorhanden</option
						><option value="missing_at_source">Nicht in Quelle</option><option value="fetch_failed"
							>Abruf fehlgeschlagen</option
						><option value="invalid">Ungültig</option><option value="storage_failed"
							>Speichern fehlgeschlagen</option
						><option value="not_checked">Nicht geprüft</option>
					</select>
				</label>
				<div class="flex flex-col justify-end gap-2 text-sm">
					<label><input type="checkbox" bind:checked={sampleOnly} /> Nur Stichprobe</label>
					<button
						class="px-3 py-2 rounded"
						style="background: var(--color-brand-500); color: white;"
					>
						Anwenden
					</button>
				</div>
			</form>

			<h3 class="text-lg font-semibold mb-3" style="color: var(--text-primary);">
				Importierte Hefte ({reviewTotal})
			</h3>
			{#if reviewLoading}
				<p>Lade Prüfergebnisse...</p>
			{:else if reviewItems.length === 0}
				<p style="color: var(--text-secondary);">Keine Ergebnisse für diese Filter.</p>
			{:else}
				<div class="overflow-x-auto">
					<table class="w-full text-sm" data-testid="issues-table">
						<thead>
							<tr style="border-bottom: 1px solid var(--border-default);">
								<th class="text-left py-3 px-2">Nr.</th><th class="text-left py-3 px-2">Ergebnis</th
								><th class="text-left py-3 px-2">Titel / Autor</th><th class="text-left py-3 px-2"
									>Datum / Teil</th
								><th class="text-left py-3 px-2">Cover</th><th class="text-left py-3 px-2"
									>Quelle</th
								>
							</tr>
						</thead>
						<tbody>
							{#each reviewItems as item (item.id)}
								<tr style="border-bottom: 1px solid var(--border-default);" data-testid="issue-row">
									<td class="py-3 px-2">{item.issue_number}</td>
									<td class="py-3 px-2">
										<strong>{item.outcome}</strong><br /><small>{item.severity}</small>
										{#if item.message}<div>{item.message}</div>{/if}
									</td>
									<td class="py-3 px-2">
										{item.title ?? '–'}<br /><small>{item.authors.join(', ') || '–'}</small>
									</td>
									<td class="py-3 px-2">
										{item.published_at ?? '–'}<br /><small
											>{item.part_number !== null && item.part_total !== null
												? `${item.part_number} von ${item.part_total}`
												: (item.cycle ?? '–')}</small
										>
									</td>
									<td class="py-3 px-2">
										{#if item.cover_local_path}
											<img
												src={item.cover_local_path}
												alt="Cover von #{item.issue_number}: {item.title ?? 'Unbekannt'}"
												class="h-16 w-auto rounded"
											/>
										{/if}
										<small
											>{coverLabel(item.cover_status)}{item.cover_reason
												? `: ${item.cover_reason}`
												: ''}</small
										>
									</td>
									<td class="py-3 px-2">
										{#if item.source_url}
											<!-- eslint-disable svelte/no-navigation-without-resolve -->
											<a href={item.source_url} target="_blank" rel="noopener noreferrer">Quelle</a>
										{:else}–{/if}
									</td>
								</tr>
							{/each}
						</tbody>
					</table>
				</div>
				{#if reviewTotal > 50}
					<div class="mt-4 flex items-center gap-3 text-sm">
						<button
							onclick={() => changeReviewPage(reviewPage - 1)}
							disabled={reviewPage <= 1}
							data-testid="previous-review-page">Zurück</button
						>
						<span>Seite {reviewPage}</span>
						<button
							onclick={() => changeReviewPage(reviewPage + 1)}
							disabled={reviewPage * 50 >= reviewTotal}
							data-testid="next-review-page">Weiter</button
						>
					</div>
				{/if}
			{/if}
		</section>
	{/if}
{/if}
