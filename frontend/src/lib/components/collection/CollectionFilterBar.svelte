<script lang="ts">
	import type { CollectionQueryParams } from '$lib/api/collection';

	interface SeriesOption {
		slug: string;
		name: string;
	}

	interface Props {
		series_options: SeriesOption[];
		onfilterchange: (params: CollectionQueryParams) => void;
	}

	let { series_options, onfilterchange }: Props = $props();

	let selectedSeries = $state('');
	let selectedStatus = $state('');
	let selectedSort = $state('issue_number');
	let selectedSortDir = $state('asc');
	let searchQuery = $state('');

	const STATUS_OPTIONS = [
		{ value: '', label: 'Alle' },
		{ value: 'owned', label: 'Vorhanden' },
		{ value: 'duplicate', label: 'Doppelt' },
		{ value: 'wanted', label: 'Gesucht' },
		{ value: 'missing', label: 'Fehlend' }
	];

	const SORT_OPTIONS = [
		{ value: 'issue_number', label: 'Heftnummer' },
		{ value: 'title', label: 'Titel' },
		{ value: 'condition', label: 'Zustand' },
		{ value: 'added', label: 'Hinzugefügt' }
	];

	function emitChange() {
		// Reset "missing" filter when no series is selected (backend requires series_slug)
		if (selectedStatus === 'missing' && !selectedSeries) {
			selectedStatus = '';
		}
		const params: CollectionQueryParams = {
			page: 1
		};
		if (selectedSeries) params.series_slug = selectedSeries;
		if (selectedStatus) params.status = selectedStatus;
		if (selectedSort && selectedSort !== 'issue_number') params.sort = selectedSort;
		if (selectedSortDir && selectedSortDir !== 'asc') params.sort_dir = selectedSortDir;
		if (searchQuery.trim()) params.q = searchQuery.trim();
		onfilterchange(params);
	}
</script>

<div
	class="glass-nav sticky top-14 z-40 px-4 py-3 flex flex-wrap gap-3 items-center"
	data-testid="collection-filter-bar"
>
	<!-- Series filter -->
	<label class="sr-only" for="filter-series">Serie</label>
	<select
		id="filter-series"
		bind:value={selectedSeries}
		onchange={emitChange}
		class="rounded-lg px-3 py-1.5 text-sm"
		style="background: var(--glass); border: 1px solid var(--glass-border); color: var(--text-primary);"
	>
		<option value="">Alle Serien</option>
		{#each series_options as s (s.slug)}
			<option value={s.slug}>{s.name}</option>
		{/each}
	</select>

	<!-- Status chips -->
	<div class="flex gap-1" role="radiogroup" aria-label="Status-Filter">
		{#each STATUS_OPTIONS as opt (opt.value)}
			{@const active = selectedStatus === opt.value}
			{@const disabled = opt.value === 'missing' && !selectedSeries}
			<button
				type="button"
				class="px-2.5 py-1 rounded-full text-xs font-medium transition-colors cursor-pointer"
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
					selectedStatus = opt.value;
					emitChange();
				}}
				data-testid={`status-filter-${opt.value || 'all'}`}
			>
				{opt.label}
			</button>
		{/each}
	</div>

	<!-- Sort -->
	<label class="sr-only" for="filter-sort">Sortierung</label>
	<select
		id="filter-sort"
		bind:value={selectedSort}
		onchange={emitChange}
		class="rounded-lg px-3 py-1.5 text-sm"
		style="background: var(--glass); border: 1px solid var(--glass-border); color: var(--text-primary);"
	>
		{#each SORT_OPTIONS as opt (opt.value)}
			<option value={opt.value}>{opt.label}</option>
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

	<!-- Search -->
	<label class="sr-only" for="filter-search">Suche</label>
	<input
		id="filter-search"
		type="search"
		placeholder="Suchen..."
		bind:value={searchQuery}
		oninput={emitChange}
		class="rounded-lg px-3 py-1.5 text-sm flex-1 min-w-[120px]"
		style="background: var(--glass); border: 1px solid var(--glass-border); color: var(--text-primary);"
		data-testid="search-input"
	/>
</div>
