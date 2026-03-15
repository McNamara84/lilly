<script lang="ts">
	import { page } from '$app/stores';
	import { resolve } from '$app/paths';
	import {
		fetchImportJob,
		fetchImportIssues,
		activateSeries,
		type ImportJob,
		type IssueAdmin
	} from '$lib/api/admin';

	let job = $state<ImportJob | null>(null);
	let issues = $state<IssueAdmin[]>([]);
	let issuesTotal = $state(0);
	let issuesPage = $state(1);
	let loading = $state(true);
	let error = $state<string | null>(null);
	let pollInterval = $state<ReturnType<typeof setInterval> | null>(null);

	const jobId = $derived(Number($page.params.id));

	async function loadJob() {
		try {
			job = await fetchImportJob(jobId);

			if (job.status === 'completed' || job.status === 'failed') {
				stopPolling();
				if (job.status === 'completed') {
					await loadIssues();
				}
			}
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load import job';
			stopPolling();
		} finally {
			loading = false;
		}
	}

	async function loadIssues() {
		try {
			const result = await fetchImportIssues(jobId, issuesPage);
			issues = result.data;
			issuesTotal = result.total;
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load issues';
		}
	}

	function startPolling() {
		pollInterval = setInterval(loadJob, 3000);
	}

	function stopPolling() {
		if (pollInterval) {
			clearInterval(pollInterval);
			pollInterval = null;
		}
	}

	function progressPercent(): number {
		if (!job || job.total_issues === 0) return 0;
		return Math.round((job.imported_issues / job.total_issues) * 100);
	}

	async function handleActivate() {
		if (!job) return;
		try {
			await activateSeries(job.series_slug);
			await loadJob();
		} catch (e) {
			error = e instanceof Error ? e.message : 'Activation failed';
		}
	}

	$effect(() => {
		loadJob();
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
	</p>

	<!-- Status & Progress -->
	<section class="glass-elevated p-6 rounded-lg mb-6" data-testid="progress-section">
		<div class="flex items-center justify-between mb-4">
			<span class="text-sm font-medium" style="color: var(--text-primary);">
				Status:
				<span
					class="inline-block px-2 py-0.5 rounded text-xs font-medium ml-1"
					class:text-green-700={job.status === 'completed'}
					class:text-red-700={job.status === 'failed'}
					class:text-yellow-700={job.status === 'running'}
					class:text-gray-700={job.status === 'pending'}
					data-testid="job-status"
				>
					{job.status}
				</span>
			</span>
			<span class="text-sm" style="color: var(--text-secondary);" data-testid="progress-count">
				{job.imported_issues} / {job.total_issues} Hefte
			</span>
		</div>

		<!-- Progress Bar -->
		<div
			class="w-full h-3 rounded-full overflow-hidden"
			style="background-color: var(--surface-base);"
			role="progressbar"
			aria-valuenow={job.imported_issues}
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
	</section>

	<!-- Completed: Show issues & activate -->
	{#if job.status === 'completed'}
		<section class="mb-6" data-testid="review-section">
			<div class="flex items-center justify-between mb-4">
				<h2 class="text-lg font-semibold" style="color: var(--text-primary);">
					Importierte Hefte ({issuesTotal})
				</h2>
				<button
					onclick={handleActivate}
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
								<th class="text-left py-3 px-2" style="color: var(--text-secondary);">Datum</th>
								<th class="text-left py-3 px-2" style="color: var(--text-secondary);">Cover</th>
							</tr>
						</thead>
						<tbody>
							{#each issues as issue (issue.id)}
								<tr style="border-bottom: 1px solid var(--border-default);" data-testid="issue-row">
									<td class="py-3 px-2" style="color: var(--text-primary);">{issue.issue_number}</td
									>
									<td class="py-3 px-2" style="color: var(--text-primary);">{issue.title}</td>
									<td class="py-3 px-2" style="color: var(--text-secondary);"
										>{issue.author ?? '–'}</td
									>
									<td class="py-3 px-2" style="color: var(--text-secondary);"
										>{issue.published_at ?? '–'}</td
									>
									<td class="py-3 px-2">
										{#if issue.cover_local_path}
											<span class="text-green-600 text-xs">&#10003;</span>
										{:else}
											<span class="text-gray-400 text-xs">–</span>
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
