<script lang="ts">
	import { getCachedProfile } from '$lib/offline/database';
	import {
		acceptConflictServerVersion,
		listConflicts,
		reapplyConflictLocalVersion
	} from '$lib/offline/collection';
	import type { StoredConflict } from '$lib/offline/types';
	import {
		formatOfflineStatusLabel,
		getOfflineStatus,
		refreshOfflineStatus,
		synchronizeNow
	} from '$lib/offline/status.svelte';
	import {
		activateWaitingServiceWorker,
		getPwaState,
		promptInstall
	} from '$lib/offline/pwa.svelte';

	const status = getOfflineStatus();
	const pwa = getPwaState();
	let showConflicts = $state(false);
	let conflicts = $state<StoredConflict[]>([]);

	let label = $derived(formatOfflineStatusLabel(status));

	async function toggleConflicts() {
		showConflicts = !showConflicts;
		if (!showConflicts) return;
		const profile = await getCachedProfile().catch(() => null);
		conflicts = profile ? await listConflicts(profile.id) : [];
	}

	async function useServer(conflict: StoredConflict) {
		await acceptConflictServerVersion(conflict);
		conflicts = conflicts.filter((item) => item.mutation_id !== conflict.mutation_id);
		await refreshOfflineStatus();
	}

	async function useLocal(conflict: StoredConflict) {
		await reapplyConflictLocalVersion(conflict);
		conflicts = conflicts.filter((item) => item.mutation_id !== conflict.mutation_id);
		await synchronizeNow();
	}
</script>

<aside
	class="fixed right-4 bottom-4 z-40 flex max-w-sm flex-col items-end gap-2"
	aria-live="polite"
>
	{#if pwa.updateAvailable}
		<div class="glass-elevated rounded-lg p-3 text-sm" data-testid="pwa-update-notice">
			<p>Eine neue App-Version ist verfügbar.</p>
			<button class="mt-2 cursor-pointer underline" onclick={activateWaitingServiceWorker}>
				Jetzt aktualisieren
			</button>
		</div>
	{/if}

	{#if pwa.canInstall}
		<button
			type="button"
			class="glass-elevated cursor-pointer rounded-full px-3 py-2 text-xs"
			onclick={promptInstall}
			data-testid="pwa-install-button"
		>
			LILLY installieren
		</button>
	{/if}

	{#if showConflicts && conflicts.length > 0}
		<div
			class="glass-elevated max-h-[60vh] overflow-auto rounded-lg p-4 text-sm"
			data-testid="conflict-panel"
		>
			<h2 class="font-semibold">Offline-Konflikte</h2>
			{#each conflicts as conflict (conflict.mutation_id)}
				<section class="mt-3 border-t pt-3" style="border-color: var(--glass-border);">
					<p>{conflict.error}</p>
					<div class="mt-2 grid grid-cols-2 gap-2 text-xs">
						<div>
							<strong>Server</strong>
							<p>{conflict.server_entry?.status ?? 'Kein Eintrag'}</p>
							<p>{conflict.server_entry?.condition_grade ?? '–'}</p>
						</div>
						<div>
							<strong>Lokal</strong>
							<p>
								{conflict.mutation.operation === 'update'
									? (conflict.mutation.changes.status ?? 'unverändert')
									: (conflict.mutation.entry.status ?? 'owned')}
							</p>
							<p>
								{conflict.mutation.operation === 'update'
									? (conflict.mutation.changes.condition_grade ?? 'unverändert')
									: (conflict.mutation.entry.condition_grade ?? '–')}
							</p>
						</div>
					</div>
					<div class="mt-2 flex flex-wrap gap-2">
						<button class="cursor-pointer underline" onclick={() => useServer(conflict)}>
							Serverstand übernehmen
						</button>
						{#if conflict.mutation.operation === 'update' && conflict.server_entry}
							<button class="cursor-pointer underline" onclick={() => useLocal(conflict)}>
								Lokalen Stand erneut anwenden
							</button>
						{/if}
					</div>
				</section>
			{/each}
		</div>
	{/if}

	<button
		type="button"
		class="glass-elevated cursor-pointer rounded-full px-3 py-2 text-xs"
		class:border-red-500={!status.online || status.conflictCount > 0 || !!status.syncError}
		onclick={status.conflictCount > 0 ? toggleConflicts : synchronizeNow}
		data-testid="offline-status"
		title={status.lastSyncedAt
			? `Letzter Datenstand: ${new Date(status.lastSyncedAt).toLocaleString('de-DE')}`
			: label}
	>
		{label}
	</button>
</aside>
