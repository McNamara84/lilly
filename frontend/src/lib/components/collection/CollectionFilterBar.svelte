<script lang="ts">
	import type { CollectionQueryParams } from '$lib/api/collection';
	import {
		CONDITION_GRADES,
		countActiveCollectionFilters,
		normalizeCollectionQuery
	} from '$lib/utils/collection-query';

	interface SeriesOption {
		slug: string;
		name: string;
	}

	interface Props {
		series_options: SeriesOption[];
		value?: CollectionQueryParams;
		onfilterchange: (params: CollectionQueryParams) => void;
	}

	let { series_options, value = {}, onfilterchange }: Props = $props();

	let selectedSeries = $state('');
	let selectedStatus = $state('');
	let selectedCondition = $state('');
	let selectedSort = $state('issue_number');
	let selectedSortDir = $state('asc');
	let issueNumber = $state('');
	let titleQuery = $state('');
	let authorQuery = $state('');
	let advancedOpen = $state(false);
	let debounceTimer: ReturnType<typeof setTimeout> | null = null;

	const activeFilterCount = $derived(
		[
			selectedSeries,
			selectedStatus,
			issueNumber,
			selectedCondition,
			titleQuery.trim(),
			authorQuery.trim()
		].filter(Boolean).length
	);
	const hasCustomState = $derived(
		activeFilterCount > 0 || selectedSort !== 'issue_number' || selectedSortDir !== 'asc'
	);
	const conditionDisabled = $derived(selectedStatus === 'missing');

	$effect(() => {
		selectedSeries = value.series_slug ?? '';
		selectedStatus = value.status ?? '';
		selectedCondition = value.condition ?? '';
		selectedSort = value.sort ?? 'issue_number';
		selectedSortDir = value.sort_dir ?? 'asc';
		issueNumber = value.issue_number ? String(value.issue_number) : '';
		titleQuery = value.title ?? '';
		authorQuery = value.author ?? '';
		if (countActiveCollectionFilters(value) > 0) advancedOpen = true;
	});

	$effect(() => {
		return () => {
			if (debounceTimer) clearTimeout(debounceTimer);
		};
	});

	function debouncedEmitChange() {
		if (debounceTimer) clearTimeout(debounceTimer);
		debounceTimer = setTimeout(emitChange, 300);
	}

	const STATUS_OPTIONS = [
		{ value: '', label: 'Alle' },
		{ value: 'owned', label: 'Vorhanden' },
		{ value: 'duplicate', label: 'Doppelt' },
		{ value: 'wanted', label: 'Gesucht' },
		{ value: 'missing', label: 'Fehlend' }
	];

	const SORT_OPTIONS = [
		{ value: 'issue_number', label: 'Heftnummer' },
		{ value: 'series', label: 'Serie' },
		{ value: 'condition', label: 'Zustand' },
		{ value: 'title', label: 'Titel' },
		{ value: 'author', label: 'Autor' },
		{ value: 'added', label: 'Hinzugefügt' }
	];

	function emitChange() {
		if (selectedStatus === 'missing' && !selectedSeries) selectedStatus = '';
		if (selectedStatus === 'missing') selectedCondition = '';

		const parsedIssueNumber = Number(issueNumber);
		const params = normalizeCollectionQuery({
			series_slug: selectedSeries || undefined,
			status: selectedStatus || undefined,
			issue_number:
				issueNumber && Number.isSafeInteger(parsedIssueNumber) && parsedIssueNumber > 0
					? parsedIssueNumber
					: undefined,
			condition: selectedCondition || undefined,
			title: titleQuery,
			author: authorQuery,
			sort: selectedSort,
			sort_dir: selectedSortDir,
			page: 1
		});
		onfilterchange(params);
	}

	function resetAll() {
		if (debounceTimer) clearTimeout(debounceTimer);
		selectedSeries = '';
		selectedStatus = '';
		selectedCondition = '';
		selectedSort = 'issue_number';
		selectedSortDir = 'asc';
		issueNumber = '';
		titleQuery = '';
		authorQuery = '';
		onfilterchange({});
	}
</script>

<div class="glass-nav sticky top-14 z-40 px-4 py-3" data-testid="collection-filter-bar">
	<div class="flex flex-wrap gap-3 items-center">
		<label class="sr-only" for="filter-series">Serie</label>
		<select
			id="filter-series"
			bind:value={selectedSeries}
			onchange={emitChange}
			class="rounded-lg px-3 py-1.5 text-sm"
			style="background: var(--glass); border: 1px solid var(--glass-border); color: var(--text-primary);"
		>
			<option value="">Alle Serien</option>
			{#each series_options as series (series.slug)}
				<option value={series.slug}>{series.name}</option>
			{/each}
		</select>

		<div class="flex gap-1 overflow-x-auto" role="radiogroup" aria-label="Status-Filter">
			{#each STATUS_OPTIONS as option (option.value)}
				{@const active = selectedStatus === option.value}
				{@const disabled = option.value === 'missing' && !selectedSeries}
				<button
					type="button"
					class="px-2.5 py-1 rounded-full text-xs font-medium transition-colors cursor-pointer whitespace-nowrap"
					class:opacity-40={disabled}
					class:cursor-not-allowed={disabled}
					style={active
						? `background: var(--color-brand-500); color: #000;`
						: `background: var(--glass); border: 1px solid var(--glass-border); color: var(--text-secondary);`}
					role="radio"
					aria-checked={active}
					aria-disabled={disabled}
					onclick={() => {
						if (disabled) return;
						selectedStatus = option.value;
						emitChange();
					}}
					data-testid={`status-filter-${option.value || 'all'}`}
				>
					{option.label}
				</button>
			{/each}
		</div>

		<label class="sr-only" for="filter-sort">Sortierung</label>
		<select
			id="filter-sort"
			bind:value={selectedSort}
			onchange={emitChange}
			class="rounded-lg px-3 py-1.5 text-sm"
			style="background: var(--glass); border: 1px solid var(--glass-border); color: var(--text-primary);"
		>
			{#each SORT_OPTIONS as option (option.value)}
				<option value={option.value}>{option.label}</option>
			{/each}
		</select>

		<button
			type="button"
			class="px-2 py-1.5 rounded-lg text-xs cursor-pointer"
			style="background: var(--glass); border: 1px solid var(--glass-border); color: var(--text-secondary);"
			aria-label={selectedSortDir === 'asc'
				? 'Aufsteigend — klicken für Absteigend'
				: 'Absteigend — klicken für Aufsteigend'}
			onclick={() => {
				selectedSortDir = selectedSortDir === 'asc' ? 'desc' : 'asc';
				emitChange();
			}}
			data-testid="sort-dir-toggle"
		>
			{selectedSortDir === 'asc' ? '↑' : '↓'}
		</button>

		<button
			type="button"
			class="sm:hidden px-3 py-1.5 rounded-lg text-xs cursor-pointer"
			style="background: var(--glass); border: 1px solid var(--glass-border); color: var(--text-secondary);"
			aria-expanded={advancedOpen}
			aria-controls="collection-metadata-filters"
			onclick={() => (advancedOpen = !advancedOpen)}
			data-testid="advanced-filter-toggle"
		>
			Filter{activeFilterCount > 0 ? ` (${activeFilterCount})` : ''}
		</button>

		{#if hasCustomState}
			<button
				type="button"
				class="px-3 py-1.5 rounded-lg text-xs cursor-pointer"
				style="color: var(--color-brand-500);"
				onclick={resetAll}
				data-testid="reset-filters"
			>
				Alle zurücksetzen
			</button>
		{/if}
	</div>

	<div
		id="collection-metadata-filters"
		class:hidden={!advancedOpen}
		class="mt-3 sm:flex grid grid-cols-1 xs:grid-cols-2 gap-3 items-end"
		data-testid="metadata-filters"
	>
		<label class="text-xs" style="color: var(--text-secondary);">
			Heftnummer
			<input
				type="number"
				min="1"
				inputmode="numeric"
				bind:value={issueNumber}
				oninput={debouncedEmitChange}
				class="block mt-1 w-full sm:w-28 rounded-lg px-3 py-1.5 text-sm"
				style="background: var(--glass); border: 1px solid var(--glass-border); color: var(--text-primary);"
				data-testid="issue-number-filter"
			/>
		</label>

		<label class="text-xs" style="color: var(--text-secondary);">
			Zustand
			<select
				bind:value={selectedCondition}
				onchange={emitChange}
				disabled={conditionDisabled}
				class="block mt-1 w-full sm:w-32 rounded-lg px-3 py-1.5 text-sm disabled:opacity-40"
				style="background: var(--glass); border: 1px solid var(--glass-border); color: var(--text-primary);"
				data-testid="condition-filter"
			>
				<option value="">Alle</option>
				{#each CONDITION_GRADES as grade (grade)}
					<option value={grade}>{grade}</option>
				{/each}
			</select>
		</label>

		<label class="text-xs flex-1" style="color: var(--text-secondary);">
			Titel
			<input
				type="search"
				bind:value={titleQuery}
				oninput={debouncedEmitChange}
				class="block mt-1 w-full rounded-lg px-3 py-1.5 text-sm"
				style="background: var(--glass); border: 1px solid var(--glass-border); color: var(--text-primary);"
				data-testid="title-filter"
			/>
		</label>

		<label class="text-xs flex-1" style="color: var(--text-secondary);">
			Autor
			<input
				type="search"
				bind:value={authorQuery}
				oninput={debouncedEmitChange}
				class="block mt-1 w-full rounded-lg px-3 py-1.5 text-sm"
				style="background: var(--glass); border: 1px solid var(--glass-border); color: var(--text-primary);"
				data-testid="author-filter"
			/>
		</label>
	</div>
</div>
