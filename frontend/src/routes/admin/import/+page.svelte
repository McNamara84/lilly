<script lang="ts">
	import {
		fetchAdapters,
		startImport,
		fetchImportHistory,
		fetchImportSchedule,
		type Adapter,
		type ImportJob,
		type ImportScheduleStatus
	} from '$lib/api/admin';
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';

	let adapters = $state<Adapter[]>([]);
	let history = $state<ImportJob[]>([]);
	let schedule = $state<ImportScheduleStatus | null>(null);
	let selectedAdapter = $state('');
	let loading = $state(true);
	let importing = $state(false);
	let error = $state<string | null>(null);

	async function loadData() {
		loading = true;
		error = null;
		try {
			const [adapterList, historyList, scheduleStatus] = await Promise.all([
				fetchAdapters(),
				fetchImportHistory(),
				fetchImportSchedule()
			]);
			adapters = adapterList;
			history = historyList;
			schedule = scheduleStatus;
			if (adapterList.length > 0 && !selectedAdapter) {
				selectedAdapter = adapterList[0].name;
			}
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load data';
		} finally {
			loading = false;
		}
	}

	function formatTrigger(trigger: ImportJob['trigger_type']): string {
		return trigger === 'scheduled' ? 'Automatisch' : 'Manuell';
	}

	async function handleStartImport() {
		if (!selectedAdapter) return;
		importing = true;
		error = null;
		try {
			const job = await startImport(selectedAdapter);
			await goto(resolve(`/admin/import/${job.id}`));
		} catch (e) {
			error = e instanceof Error ? e.message : 'Import failed';
			importing = false;
		}
	}

	function formatDate(dateStr: string | null): string {
		if (!dateStr) return '–';
		return new Date(dateStr).toLocaleString('de-DE', { timeZone: 'Europe/Berlin' });
	}

	$effect(() => {
		loadData();
	});
</script>

<svelte:head>
	<title>Import – LILLY Admin</title>
</svelte:head>

<h1
	class="text-2xl font-bold mb-6"
	style="color: var(--text-primary);"
	data-testid="admin-import-title"
>
	Import
</h1>

{#if loading}
	<p style="color: var(--text-secondary);" data-testid="loading-indicator">Lade...</p>
{:else}
	{#if error}
		<div
			class="p-4 rounded-lg mb-4"
			style="background-color: var(--color-error-100); color: var(--color-error-700);"
			role="alert"
			data-testid="error-message"
		>
			{error}
		</div>
	{/if}

	{#if schedule}
		<section class="glass-elevated p-6 rounded-lg mb-8" data-testid="schedule-section">
			<h2 class="text-lg font-semibold mb-2" style="color: var(--text-primary);">
				Automatischer Import
			</h2>
			{#if schedule.enabled}
				<p class="text-sm" style="color: var(--text-secondary);" data-testid="schedule-status">
					Aktiv für {schedule.adapters.join(', ')}. Nächster Lauf: {formatDate(schedule.next_run)}
					({schedule.timezone}).
				</p>
			{:else}
				<p class="text-sm" style="color: var(--text-secondary);" data-testid="schedule-status">
					Deaktiviert. Geplanter Termin: samstags um 06:10 Uhr ({schedule.timezone}).
				</p>
			{/if}
		</section>
	{/if}

	<!-- Start Import Section -->
	<section class="glass-elevated p-6 rounded-lg mb-8" data-testid="start-import-section">
		<h2 class="text-lg font-semibold mb-4" style="color: var(--text-primary);">
			Neuen Import starten
		</h2>
		<div class="flex flex-col sm:flex-row gap-4 items-start sm:items-end">
			<div class="flex-1">
				<label
					for="adapter-select"
					class="block text-sm mb-1"
					style="color: var(--text-secondary);"
				>
					Adapter auswählen
				</label>
				<select
					id="adapter-select"
					bind:value={selectedAdapter}
					class="w-full px-3 py-2 rounded-lg text-sm"
					style="background-color: var(--surface-base); color: var(--text-primary); border: 1px solid var(--border-default);"
					data-testid="adapter-select"
				>
					{#each adapters as adapter (adapter.name)}
						<option value={adapter.name}>
							{adapter.display_name} (v{adapter.version})
						</option>
					{/each}
				</select>
			</div>
			<button
				onclick={handleStartImport}
				disabled={importing || !selectedAdapter}
				class="px-4 py-2 rounded-lg text-sm font-medium cursor-pointer transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
				style="background-color: var(--color-brand-500); color: white;"
				data-testid="start-import-button"
			>
				{importing ? 'Wird gestartet...' : 'Import starten'}
			</button>
		</div>
	</section>

	<!-- Import History -->
	<section data-testid="import-history-section">
		<h2 class="text-lg font-semibold mb-4" style="color: var(--text-primary);">Import-Historie</h2>

		{#if history.length === 0}
			<p style="color: var(--text-secondary);" data-testid="empty-history">
				Noch keine Imports durchgeführt.
			</p>
		{:else}
			<div class="overflow-x-auto">
				<table class="w-full text-sm" data-testid="history-table">
					<thead>
						<tr style="border-bottom: 1px solid var(--border-default);">
							<th class="text-left py-3 px-2" style="color: var(--text-secondary);">Adapter</th>
							<th class="text-left py-3 px-2" style="color: var(--text-secondary);">Status</th>
							<th class="text-left py-3 px-2" style="color: var(--text-secondary);">Auslöser</th>
							<th class="text-center py-3 px-2" style="color: var(--text-secondary);"
								>Fortschritt</th
							>
							<th class="text-left py-3 px-2" style="color: var(--text-secondary);">Gestartet</th>
							<th class="text-right py-3 px-2" style="color: var(--text-secondary);">Details</th>
						</tr>
					</thead>
					<tbody>
						{#each history as job (job.id)}
							<tr style="border-bottom: 1px solid var(--border-default);" data-testid="history-row">
								<td class="py-3 px-2" style="color: var(--text-primary);">
									{job.adapter_name}
								</td>
								<td class="py-3 px-2">
									<span
										class="inline-block px-2 py-0.5 rounded text-xs font-medium"
										class:text-green-700={job.status === 'completed'}
										class:text-orange-700={job.status === 'completed_with_errors'}
										class:text-red-700={job.status === 'failed'}
										class:text-yellow-700={job.status === 'running'}
										class:text-gray-700={job.status === 'pending'}
									>
										{job.status}
									</span>
								</td>
								<td class="py-3 px-2" style="color: var(--text-secondary);">
									{formatTrigger(job.trigger_type)}
								</td>
								<td class="py-3 px-2 text-center" style="color: var(--text-secondary);">
									{job.imported_issues + job.failed_issues} / {job.total_issues}
								</td>
								<td class="py-3 px-2" style="color: var(--text-secondary);">
									{formatDate(job.started_at)}
								</td>
								<td class="py-3 px-2 text-right">
									<a
										href={resolve(`/admin/import/${job.id}`)}
										class="text-xs px-3 py-1.5 rounded-lg transition-colors"
										style="color: var(--color-brand-500);"
										data-testid="view-details-link"
									>
										Details
									</a>
								</td>
							</tr>
						{/each}
					</tbody>
				</table>
			</div>
		{/if}
	</section>
{/if}
