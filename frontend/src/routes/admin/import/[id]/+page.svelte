<script lang="ts">
	import { page } from '$app/stores';
	import { resolve } from '$app/paths';
	import { goto } from '$app/navigation';
	import {
		fetchImportJob,
		fetchImportSeriesIssues,
		fetchImportErrors,
		cancelImport,
		retryImport,
		activateSeries,
		type ImportJob,
		type ImportJobError,
		type IssueAdmin
	} from '$lib/api/admin';

	let job = $state<ImportJob | null>(null);
	let issues = $state<IssueAdmin[]>([]);
	let issuesTotal = $state(0);
	let issuesPage = $state(1);
	let jobErrors = $state<ImportJobError[]>([]);
	let jobErrorsTotal = $state(0);
	let jobErrorsPage = $state(1);
	let loading = $state(true);
	let error = $state<string | null>(null);
	let actionPending = $state(false);
	let polling = false;
	let pollTimeout: ReturnType<typeof setTimeout> | null = null;

	const jobId = $derived(Number($page.params.id));
	const invalidJobId = $derived(!Number.isFinite(jobId) || jobId < 1);

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
				if (job.status === 'completed' || job.status === 'completed_with_errors') {
					await loadIssues();
				}
				await loadErrors();
			}
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load import job';
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
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load import errors';
		}
	}

	function isTerminal(status: ImportJob['status']): boolean {
		return !['pending', 'running'].includes(status);
	}

	function canRetry(status: ImportJob['status']): boolean {
		return ['failed', 'cancelled', 'interrupted'].includes(status);
	}

	function formatDate(date: string | undefined | null): string {
		return date ? new Date(date).toLocaleString('de-DE') : '–';
	}

	async function changeErrorsPage(pageNumber: number) {
		jobErrorsPage = pageNumber;
		await loadErrors();
	}

	async function loadIssues() {
		try {
			const result = await fetchImportSeriesIssues(jobId, issuesPage);
			issues = result.data;
			issuesTotal = result.total;
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load issues';
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
		if (pollTimeout) {
			clearTimeout(pollTimeout);
			pollTimeout = null;
		}
	}

	function resetPageState() {
		stopPolling();
		job = null;
		issues = [];
		issuesTotal = 0;
		issuesPage = 1;
		jobErrors = [];
		jobErrorsTotal = 0;
		jobErrorsPage = 1;
		loading = true;
		error = null;
		actionPending = false;
	}

	function progressPercent(): number {
		if (!job || job.total_issues === 0) return 0;
		return Math.round((processedCount() / job.total_issues) * 100);
	}

	function processedCount(): number {
		if (!job) return 0;
		return job.imported_issues + (job.skipped_issues ?? 0) + job.failed_issues;
	}

	async function handleActivate(seriesSlug: string) {
		try {
			await activateSeries(seriesSlug);
			await loadJob();
		} catch (e) {
			error = e instanceof Error ? e.message : 'Activation failed';
		}
	}

	async function handleCancel() {
		if (!job) return;
		actionPending = true;
		error = null;
		try {
			job = await cancelImport(job.id);
		} catch (e) {
			error = e instanceof Error ? e.message : 'Cancellation failed';
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
		} catch (e) {
			error = e instanceof Error ? e.message : 'Retry failed';
		} finally {
			actionPending = false;
		}
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
	data-testid="back-link"
>
	&larr; Zurück zur Import-Übersicht
</a>

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

	<!-- Status & Progress -->
	<section class="glass-elevated p-6 rounded-lg mb-6" data-testid="progress-section">
		<div class="flex items-center justify-between mb-4">
			<span class="text-sm font-medium" style="color: var(--text-primary);">
				Status:
				<span
					class="inline-block px-2 py-0.5 rounded text-xs font-medium ml-1"
					class:text-green-700={job.status === 'completed'}
					class:text-orange-700={job.status === 'completed_with_errors'}
					class:text-red-700={job.status === 'failed' || job.status === 'interrupted'}
					class:text-gray-700={job.status === 'cancelled' || job.status === 'pending'}
					class:text-yellow-700={job.status === 'running'}
					data-testid="job-status"
				>
					{job.status}
				</span>
			</span>
			<span class="text-sm" style="color: var(--text-secondary);" data-testid="progress-count">
				{processedCount()} / {job.total_issues} bearbeitet ({job.created_issues ?? 0} neu,
				{job.updated_issues ?? 0} geändert, {job.unchanged_issues ?? 0} unverändert,
				{job.skipped_issues ?? 0} übersprungen, {job.failed_issues} fehlgeschlagen)
			</span>
		</div>

		<!-- Progress Bar -->
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
					class="px-4 py-2 rounded-lg text-sm font-medium cursor-pointer disabled:opacity-50"
					style="background-color: var(--color-error-500); color: white;"
					data-testid="cancel-import-button"
				>
					{job.cancel_requested_at ? 'Abbruch angefordert' : 'Import abbrechen'}
				</button>
			{:else if canRetry(job.status)}
				<button
					onclick={handleRetry}
					disabled={actionPending}
					class="px-4 py-2 rounded-lg text-sm font-medium cursor-pointer disabled:opacity-50"
					style="background-color: var(--color-brand-500); color: white;"
					data-testid="retry-import-button"
				>
					Erneut vollständig synchronisieren
				</button>
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
						{item.message}
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

	<!-- Completed: Show issues & activate -->
	{#if job.status === 'completed' || job.status === 'completed_with_errors'}
		<section class="mb-6" data-testid="review-section">
			<div class="flex items-center justify-between mb-4">
				<h2 class="text-lg font-semibold" style="color: var(--text-primary);">
					Importierte Hefte ({issuesTotal})
				</h2>
				<button
					onclick={() => handleActivate(job!.series_slug)}
					class="px-4 py-2 rounded-lg text-sm font-medium cursor-pointer transition-colors"
					style="background-color: var(--color-success-500); color: white;"
					data-testid="activate-series-button"
				>
					Serie aktivieren
				</button>
			</div>

			{#if issues.length > 0}
				<div class="overflow-x-auto">
					<table class="w-full text-sm" data-testid="issues-table">
						<thead>
							<tr style="border-bottom: 1px solid var(--border-default);">
								<th class="text-left py-3 px-2" style="color: var(--text-secondary);">Nr.</th>
								<th class="text-left py-3 px-2" style="color: var(--text-secondary);">Titel</th>
								<th class="text-left py-3 px-2" style="color: var(--text-secondary);">Autor</th>
								<th class="text-left py-3 px-2" style="color: var(--text-secondary);">Zyklus</th>
								<th class="text-left py-3 px-2" style="color: var(--text-secondary);">Datum</th>
								<th class="text-left py-3 px-2" style="color: var(--text-secondary);">Teil</th>
								<th class="text-left py-3 px-2" style="color: var(--text-secondary);">Zeichner</th>
								<th class="text-left py-3 px-2" style="color: var(--text-secondary);">Cover</th>
								<th class="text-left py-3 px-2" style="color: var(--text-secondary);">Quelle</th>
							</tr>
						</thead>
						<tbody>
							{#each issues as issue (issue.id)}
								<tr style="border-bottom: 1px solid var(--border-default);" data-testid="issue-row">
									<td class="py-3 px-2" style="color: var(--text-primary);">{issue.issue_number}</td
									>
									<td class="py-3 px-2" style="color: var(--text-primary);">{issue.title}</td>
									<td class="py-3 px-2" style="color: var(--text-secondary);"
										>{issue.authors.length > 0 ? issue.authors.join(', ') : '–'}</td
									>
									<td class="py-3 px-2" style="color: var(--text-secondary);"
										>{issue.cycle ?? '–'}</td
									>
									<td class="py-3 px-2" style="color: var(--text-secondary);"
										>{issue.published_at ?? '–'}</td
									>
									<td class="py-3 px-2" style="color: var(--text-secondary);">
										{issue.part_number !== null && issue.part_total !== null
											? `${issue.part_number} von ${issue.part_total}`
											: '–'}
									</td>
									<td class="py-3 px-2" style="color: var(--text-secondary);"
										>{issue.cover_artists.length > 0 ? issue.cover_artists.join(', ') : '–'}</td
									>
									<td class="py-3 px-2">
										{#if issue.cover_local_path || issue.cover_url}
											<img
												src={issue.cover_local_path ?? issue.cover_url}
												alt="Cover von #{issue.issue_number}: {issue.title}"
												class="h-16 w-auto rounded"
											/>
										{:else}
											<span class="text-gray-400 text-xs">–</span>
										{/if}
									</td>
									<td class="py-3 px-2">
										{#if issue.source_wiki_url}
											<!-- eslint-disable svelte/no-navigation-without-resolve -->
											<a
												href={issue.source_wiki_url}
												target="_blank"
												rel="noopener noreferrer"
												style="color: var(--color-brand-500);">Quelle</a
											>
										{:else}
											<span style="color: var(--text-secondary);">–</span>
										{/if}
									</td>
								</tr>
							{/each}
						</tbody>
					</table>
				</div>
			{/if}
		</section>
	{/if}
{/if}
