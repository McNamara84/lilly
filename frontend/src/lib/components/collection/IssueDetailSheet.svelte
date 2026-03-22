<script lang="ts">
	import type { CollectionEntry } from '$lib/api/collection';
	import type { Issue } from '$lib/api/series';
	import ConditionChips from './ConditionChips.svelte';

	interface Props {
		issue: Issue | null;
		collection_entry: CollectionEntry | null;
		onclose: () => void;
		onsave: (data: {
			issue_id: number;
			condition_grade: string;
			status?: string;
			notes?: string;
		}) => void;
		ondelete?: () => void;
	}

	let { issue, collection_entry, onclose, onsave, ondelete }: Props = $props();

	let conditionGrade = $state('Z2');
	let status = $state('owned');
	let notes = $state('');

	// Sync state when props change
	$effect(() => {
		conditionGrade = collection_entry?.condition_grade ?? 'Z2';
		status = collection_entry?.status ?? 'owned';
		notes = collection_entry?.notes ?? '';
	});

	const isOpen = $derived(issue !== null);
	const isEditing = $derived(collection_entry !== null && collection_entry.id > 0);

	function handleSave() {
		if (!issue) return;
		onsave({
			issue_id: issue.id,
			condition_grade: conditionGrade,
			status,
			notes: notes.trim() || undefined
		});
	}

	function handleKeydown(e: KeyboardEvent) {
		if (e.key === 'Escape') onclose();
	}
</script>

{#if isOpen && issue}
	<!-- Backdrop -->
	<div
		class="fixed inset-0 z-50 bg-black/50"
		onclick={onclose}
		onkeydown={handleKeydown}
		role="button"
		tabindex="-1"
		aria-label="Sheet schließen"
		data-testid="detail-sheet-backdrop"
	></div>

	<!-- Sheet -->
	<!-- svelte-ignore a11y_no_noninteractive_element_to_interactive_role -->
	<aside
		class="fixed z-50 overflow-y-auto
			bottom-0 left-0 right-0 max-h-[85vh] rounded-t-2xl
			md:top-0 md:right-0 md:bottom-0 md:left-auto md:w-[420px] md:max-h-full md:rounded-t-none md:rounded-l-2xl"
		style="background: var(--surface-raised); border: 1px solid var(--glass-border);"
		role="dialog"
		aria-modal="true"
		aria-label="Heftdetails: {issue.title}"
		data-testid="issue-detail-sheet"
	>
		<!-- Drag handle (mobile) -->
		<div class="flex justify-center py-2 md:hidden">
			<div class="w-10 h-1 rounded-full" style="background: var(--glass-border);"></div>
		</div>

		<div class="p-5">
			<!-- Header -->
			<div class="flex gap-4 mb-6">
				{#if issue.cover_url || issue.cover_local_path}
					<img
						src={issue.cover_local_path ?? issue.cover_url}
						alt="Cover von #{issue.issue_number}: {issue.title}"
						class="w-24 h-36 object-cover rounded-lg flex-shrink-0"
					/>
				{:else}
					<div
						class="w-24 h-36 rounded-lg flex items-center justify-center flex-shrink-0"
						style="background: var(--glass-border); color: var(--text-tertiary);"
					>
						#{issue.issue_number}
					</div>
				{/if}
				<div>
					<p class="text-xs" style="color: var(--text-tertiary);">#{issue.issue_number}</p>
					<h2 class="text-lg font-bold" style="color: var(--text-primary);">{issue.title}</h2>
					{#if issue.authors?.length}
						<p class="text-sm mt-1" style="color: var(--text-secondary);">
							{issue.authors.join(', ')}
						</p>
					{/if}
					{#if issue.cycle}
						<p class="text-xs mt-1" style="color: var(--text-tertiary);">
							Zyklus: {issue.cycle}
						</p>
					{/if}
				</div>
			</div>

			<!-- Collection actions -->
			<section class="mb-6">
				<h3 class="text-sm font-semibold mb-3" style="color: var(--text-primary);">
					Sammlungsstatus
				</h3>

				<!-- Status toggle -->
				<div class="flex gap-2 mb-4" role="radiogroup" aria-label="Status">
					{#each ['owned', 'duplicate', 'wanted'] as s (s)}
						{@const isActive = status === s}
						{@const label = s === 'owned' ? 'Vorhanden' : s === 'duplicate' ? 'Doppelt' : 'Gesucht'}
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
							data-testid={`status-${s}`}
						>
							{label}
						</button>
					{/each}
				</div>

				<!-- Condition -->
				<p class="text-xs mb-2" style="color: var(--text-tertiary);">Zustand</p>
				<ConditionChips value={conditionGrade} onchange={(g) => (conditionGrade = g)} />

				<!-- Notes -->
				<label
					for="entry-notes"
					class="block text-xs mt-4 mb-1"
					style="color: var(--text-tertiary);"
				>
					Notizen
				</label>
				<textarea
					id="entry-notes"
					bind:value={notes}
					rows="3"
					class="w-full rounded-lg p-2 text-sm resize-none"
					style="background: var(--glass); border: 1px solid var(--glass-border); color: var(--text-primary);"
					placeholder="Optionale Notizen..."
					data-testid="notes-textarea"
				></textarea>
			</section>

			<!-- Actions -->
			<div class="flex gap-3">
				<button
					type="button"
					class="flex-1 rounded-lg px-4 py-2.5 text-sm font-semibold cursor-pointer transition-colors"
					style="background: var(--color-brand-500); color: #000;"
					onclick={handleSave}
					data-testid="save-button"
				>
					{isEditing ? 'Speichern' : 'Hinzufügen'}
				</button>
				{#if isEditing && ondelete}
					<button
						type="button"
						class="rounded-lg px-4 py-2.5 text-sm cursor-pointer transition-colors"
						style="background: var(--glass); border: 1px solid var(--color-error); color: var(--color-error);"
						onclick={ondelete}
						data-testid="delete-button"
					>
						Entfernen
					</button>
				{/if}
			</div>

			<!-- Wiki link -->
			{#if issue.source_wiki_url}
				<!-- eslint-disable svelte/no-navigation-without-resolve -->
				<a
					href={issue.source_wiki_url}
					target="_blank"
					rel="noopener noreferrer"
					class="block text-center text-xs mt-4 underline"
					style="color: var(--text-tertiary);"
					data-testid="wiki-link"
				>
					Im Wiki ansehen ↗
				</a>
			{/if}
		</div>
	</aside>
{/if}

<svelte:window onkeydown={isOpen ? handleKeydown : undefined} />
