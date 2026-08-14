<script lang="ts">
	import { page } from '$app/stores';
	import { untrack } from 'svelte';
	import { getAuthState } from '$lib/stores/auth.svelte';
	import { fetchIssue, type Issue } from '$lib/api/series';
	import {
		fetchCollectionEntriesByIssue,
		addToCollection,
		updateCollectionEntry,
		deleteCollectionEntry,
		type CollectionEntry,
		type PersistedCollectionStatus
	} from '$lib/api/collection';
	import type { ConditionGrade } from '$lib/collection/conditions';
	import {
		MAX_COLLECTION_NOTE_LENGTH,
		countCollectionNoteCharacters,
		limitCollectionNote
	} from '$lib/collection/notes';
	import ConditionChips from '$lib/components/collection/ConditionChips.svelte';
	import PhotoUploader from '$lib/components/media/PhotoUploader.svelte';
	import {
		MAX_EDITION_LABEL_LENGTH,
		countEditionLabelCharacters,
		limitEditionLabel
	} from '$lib/collection/editions';

	const auth = getAuthState();

	let issue = $state<Issue | null>(null);
	let entry = $state<CollectionEntry | null>(null);
	let entries = $state<CollectionEntry[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);
	let saving = $state(false);

	// Editable fields
	let conditionGrade = $state<ConditionGrade>('Z2');
	let status = $state<PersistedCollectionStatus>('owned');
	let notes = $state('');
	let editionLabel = $state('');
	const ENTRY_STATUSES = ['owned', 'duplicate', 'wanted'] as const;

	const issueId = $derived(Number($page.params.id));
	const isInCollection = $derived(entry !== null && entry.id > 0);
	const needsCondition = $derived(status !== 'wanted' || entry?.condition_grade != null);
	const noteLength = $derived(countCollectionNoteCharacters(notes));
	const editionLabelLength = $derived(countEditionLabelCharacters(editionLabel));

	function selectEntry(selected: CollectionEntry | null) {
		entry = selected;
		conditionGrade = selected?.condition_grade ?? 'Z2';
		status = selected?.status === 'missing' || !selected ? 'owned' : selected.status;
		notes = selected?.notes ?? '';
		editionLabel = selected?.edition_label ?? '';
	}

	$effect(() => {
		const currentIssueId = issueId;
		const authenticationLoading = auth.isLoading;
		const authenticated = auth.isAuthenticated;
		if (authenticationLoading) return;
		untrack(() => {
			loadIssue(currentIssueId, authenticated);
		});
	});

	async function loadIssue(currentIssueId: number, authenticated: boolean) {
		if (!currentIssueId || isNaN(currentIssueId)) {
			issue = null;
			entry = null;
			error = 'Ungültige Heft-ID';
			loading = false;
			return;
		}
		loading = true;
		error = null;
		issue = null;
		// Reset collection state before lookup to avoid stale data
		entry = null;
		entries = [];
		conditionGrade = 'Z2';
		status = 'owned';
		notes = '';
		editionLabel = '';
		try {
			issue = await fetchIssue(currentIssueId);

			// Try to load collection entry for this issue (if authenticated)
			if (authenticated) {
				try {
					entries = await fetchCollectionEntriesByIssue(currentIssueId);
					selectEntry(entries[0] ?? null);
				} catch {
					// Not critical if collection lookup fails
				}
			}
		} catch (e) {
			error = e instanceof Error ? e.message : 'Heft nicht gefunden';
		} finally {
			loading = false;
		}
	}

	async function handleAdd() {
		if (!issue) return;
		saving = true;
		try {
			const created = await addToCollection({
				issue_id: issue.id,
				condition_grade: needsCondition ? conditionGrade : undefined,
				status,
				notes,
				edition_label: editionLabel
			});
			entries = [...entries, created];
			selectEntry(created);
		} catch (e) {
			error = e instanceof Error ? e.message : 'Fehler beim Hinzufügen';
		} finally {
			saving = false;
		}
	}

	async function handleUpdate() {
		if (!entry || entry.id <= 0) return;
		saving = true;
		try {
			const updated = await updateCollectionEntry(entry.id, {
				condition_grade: needsCondition ? conditionGrade : undefined,
				status,
				notes,
				edition_label: editionLabel
			});
			entries = entries.map((candidate) => (candidate.id === updated.id ? updated : candidate));
			selectEntry(updated);
		} catch (e) {
			error = e instanceof Error ? e.message : 'Fehler beim Speichern';
		} finally {
			saving = false;
		}
	}

	async function handleRemove() {
		if (!entry || entry.id <= 0) return;
		saving = true;
		try {
			await deleteCollectionEntry(entry.id);
			entries = entries.filter((candidate) => candidate.id !== entry?.id);
			selectEntry(entries[0] ?? null);
		} catch (e) {
			error = e instanceof Error ? e.message : 'Fehler beim Entfernen';
		} finally {
			saving = false;
		}
	}

	function handleNotesInput(event: Event) {
		const textarea = event.currentTarget as HTMLTextAreaElement;
		notes = limitCollectionNote(textarea.value);
		if (textarea.value !== notes) textarea.value = notes;
	}

	function handleEditionInput(event: Event) {
		const input = event.currentTarget as HTMLInputElement;
		editionLabel = limitEditionLabel(input.value);
		if (input.value !== editionLabel) input.value = editionLabel;
	}
</script>

<svelte:head>
	<title>{issue ? `${issue.title} – LILLY` : 'Heft – LILLY'}</title>
</svelte:head>

<div class="min-h-[calc(100vh-3.5rem)] px-4 py-8 sm:px-6 lg:px-8">
	{#if loading}
		<p data-testid="loading-indicator">Lade Heft...</p>
	{:else if error}
		<div role="alert" data-testid="error-message">
			<p style="color: var(--color-error);">{error}</p>
		</div>
	{:else if issue}
		<div class="max-w-4xl mx-auto">
			<!-- Header -->
			<header class="flex flex-col sm:flex-row gap-6 mb-8" data-testid="issue-detail-header">
				{#if issue.cover_url || issue.cover_local_path}
					<img
						src={issue.cover_local_path ?? issue.cover_url}
						alt="Cover von #{issue.issue_number}: {issue.title}"
						class="w-48 h-72 object-cover rounded-lg flex-shrink-0"
						data-testid="issue-detail-cover"
					/>
				{:else}
					<div
						class="w-48 h-72 rounded-lg flex items-center justify-center flex-shrink-0"
						style="background: var(--glass-border); color: var(--text-tertiary);"
						data-testid="issue-detail-cover"
					>
						#{issue.issue_number}
					</div>
				{/if}

				<div>
					<p class="text-sm" style="color: var(--text-tertiary);">
						Heft #{issue.issue_number}
					</p>
					<h1 class="text-3xl font-bold mb-2" style="color: var(--text-primary);">
						{issue.title}
					</h1>
					{#if issue.authors?.length}
						<p class="text-sm mb-1" style="color: var(--text-secondary);">
							{issue.authors.join(', ')}
						</p>
					{/if}
					{#if issue.published_at}
						<p class="text-sm" style="color: var(--text-tertiary);">
							{new Date(issue.published_at).toLocaleDateString('de-DE')}
						</p>
					{/if}
					{#if issue.cycle}
						<p class="text-sm mt-1" style="color: var(--text-tertiary);">
							Zyklus: {issue.cycle}
						</p>
					{/if}
					{#if issue.keywords?.length}
						<div class="flex flex-wrap gap-1 mt-2">
							{#each issue.keywords as kw (kw)}
								<span
									class="px-2 py-0.5 text-xs rounded-full"
									style="background: var(--glass); border: 1px solid var(--glass-border); color: var(--text-secondary);"
								>
									{kw}
								</span>
							{/each}
						</div>
					{/if}
				</div>
			</header>

			<!-- Collection panel (only when authenticated) -->
			{#if auth.isAuthenticated}
				<section
					class="glass-elevated rounded-lg p-6 mb-6"
					data-testid="issue-detail-collection-panel"
				>
					<h2 class="text-lg font-semibold mb-4" style="color: var(--text-primary);">
						{isInCollection ? 'In deiner Sammlung' : 'Zur Sammlung hinzufügen'}
					</h2>

					{#if entries.length > 0}
						<div class="mb-5" data-testid="issue-edition-list">
							<h3 class="mb-2 text-sm font-semibold">Exemplare und Editionen</h3>
							<div class="flex flex-wrap gap-2">
								{#each entries as candidate (candidate.id)}
									<button
										type="button"
										aria-pressed={entry?.id === candidate.id}
										onclick={() => selectEntry(candidate)}
										class="cursor-pointer rounded-lg px-3 py-2 text-left text-xs"
										style={entry?.id === candidate.id
											? 'background: var(--color-brand-500); color: #000;'
											: 'background: var(--glass); color: var(--text-secondary);'}
									>
										<span class="block font-semibold">
											{candidate.edition_label ?? 'Edition nicht angegeben'}
										</span>
										<span>Exemplar {candidate.copy_number ?? '–'}</span>
									</button>
								{/each}
								<button
									type="button"
									onclick={() => selectEntry(null)}
									class="cursor-pointer rounded-lg px-3 py-2 text-xs"
									style="border: 1px dashed var(--glass-border); color: var(--text-secondary);"
								>
									+ Weiteres Exemplar
								</button>
							</div>
						</div>
					{/if}

					<!-- Status toggle -->
					<div class="flex gap-2 mb-4" role="radiogroup" aria-label="Status">
						{#each ENTRY_STATUSES as s (s)}
							{@const isActive = status === s}
							{@const label =
								s === 'owned' ? 'Vorhanden' : s === 'duplicate' ? 'Doppelt' : 'Gesucht'}
							{@const color = `var(--color-status-${s})`}
							<button
								type="button"
								class="flex-1 px-3 py-2 rounded-lg text-sm font-medium transition-colors cursor-pointer"
								style={isActive
									? `background-color: ${color}; color: #000;`
									: `background: var(--glass); border: 1px solid var(--glass-border); color: var(--text-secondary);`}
								role="radio"
								aria-checked={isActive}
								onclick={() => (status = s)}
							>
								{label}
							</button>
						{/each}
					</div>

					<!-- Condition -->
					{#if needsCondition}
						<p class="text-xs mb-2" style="color: var(--text-tertiary);">Zustand</p>
						<ConditionChips value={conditionGrade} onchange={(g) => (conditionGrade = g)} />
					{:else}
						<p
							class="text-xs"
							style="color: var(--text-tertiary);"
							data-testid="wanted-condition-hint"
						>
							Ein Zustand wird erst benötigt, sobald du ein Exemplar besitzt oder anbietest.
						</p>
					{/if}

					<label
						for="detail-edition"
						class="block text-xs mt-4 mb-1"
						style="color: var(--text-tertiary);"
					>
						Edition oder Auflage
					</label>
					<input
						id="detail-edition"
						value={editionLabel}
						oninput={handleEditionInput}
						aria-describedby="detail-edition-hint"
						class="w-full rounded-lg p-2 text-sm"
						style="background: var(--glass); border: 1px solid var(--glass-border); color: var(--text-primary);"
						placeholder="z. B. 1. Auflage oder Variantcover 2024"
						data-testid="detail-edition-input"
					/>
					<p id="detail-edition-hint" class="mt-1 text-[10px]" style="color: var(--text-tertiary);">
						Optional; leer lassen für eine beliebige oder unbekannte Edition. {editionLabelLength}/{MAX_EDITION_LABEL_LENGTH}
					</p>

					<!-- Notes -->
					<label
						for="detail-notes"
						class="block text-xs mt-4 mb-1"
						style="color: var(--text-tertiary);"
					>
						Notizen
					</label>
					<textarea
						id="detail-notes"
						value={notes}
						oninput={handleNotesInput}
						rows="3"
						aria-describedby="detail-notes-hint"
						class="w-full rounded-lg p-2 text-sm resize-none"
						style="background: var(--glass); border: 1px solid var(--glass-border); color: var(--text-primary);"
						placeholder="Optionale Notizen..."></textarea>
					<p id="detail-notes-hint" class="mt-1 text-[10px]" style="color: var(--text-tertiary);">
						Leeren und speichern entfernt die Notiz. {noteLength}/{MAX_COLLECTION_NOTE_LENGTH}
					</p>

					<!-- Actions -->
					<div class="flex gap-3 mt-4">
						{#if isInCollection}
							<button
								type="button"
								class="flex-1 rounded-lg px-4 py-2.5 text-sm font-semibold cursor-pointer"
								style="background: var(--color-brand-500); color: #000;"
								disabled={saving}
								onclick={handleUpdate}
							>
								{saving ? 'Speichern...' : 'Speichern'}
							</button>
							<button
								type="button"
								class="rounded-lg px-4 py-2.5 text-sm cursor-pointer"
								style="background: var(--glass); border: 1px solid var(--color-error); color: var(--color-error);"
								disabled={saving}
								onclick={handleRemove}
							>
								Entfernen
							</button>
						{:else}
							<button
								type="button"
								class="flex-1 rounded-lg px-4 py-2.5 text-sm font-semibold cursor-pointer"
								style="background: var(--color-brand-500); color: #000;"
								disabled={saving}
								onclick={handleAdd}
							>
								{saving ? 'Hinzufügen...' : 'Zur Sammlung hinzufügen'}
							</button>
						{/if}
					</div>

					{#if isInCollection && entry && entry.status !== 'wanted' && status !== 'wanted'}
						<PhotoUploader entryId={entry.id} />
					{:else if isInCollection && (entry?.status === 'wanted' || status === 'wanted')}
						<p class="mt-4 text-xs" style="color: var(--text-tertiary);">
							Eigene Fotos sind für vorhandene oder doppelte Exemplare verfügbar.
						</p>
					{:else}
						<p class="mt-4 text-xs" style="color: var(--text-tertiary);">
							Speichere das Exemplar zuerst, um eigene Fotos hinzuzufügen.
						</p>
					{/if}
				</section>
			{/if}

			<!-- Trade availability placeholder -->
			<section class="glass-elevated rounded-lg p-6 mb-6">
				<h2 class="text-lg font-semibold mb-2" style="color: var(--text-primary);">
					Tausch-Verfügbarkeit
				</h2>
				<p class="text-sm" style="color: var(--text-tertiary);">
					Noch keine Tausch-Daten verfügbar.
				</p>
			</section>

			<!-- Wiki link -->
			{#if issue.source_wiki_url}
				<!-- eslint-disable svelte/no-navigation-without-resolve -->
				<a
					href={issue.source_wiki_url}
					target="_blank"
					rel="noopener noreferrer"
					class="inline-block text-sm underline"
					style="color: var(--color-brand-500);"
					data-testid="issue-detail-wiki-link"
				>
					Im Wiki ansehen ↗
				</a>
			{/if}
		</div>
	{/if}
</div>
