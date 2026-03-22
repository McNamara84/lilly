import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/svelte';
import { userEvent } from '@testing-library/user-event';
import CollectionFilterBar from '../src/lib/components/collection/CollectionFilterBar.svelte';
import type { CollectionQueryParams } from '$lib/api/collection';

describe('CollectionFilterBar', () => {
	let onfilterchange: (params: CollectionQueryParams) => void;

	const seriesOptions = [
		{ slug: 'maddrax', name: 'Maddrax' },
		{ slug: 'perry-rhodan', name: 'Perry Rhodan' }
	];

	beforeEach(() => {
		onfilterchange = vi.fn();
	});

	it('renders the filter bar', () => {
		render(CollectionFilterBar, {
			props: { series_options: seriesOptions, onfilterchange }
		});

		expect(screen.getByTestId('collection-filter-bar')).toBeInTheDocument();
	});

	it('renders series dropdown with options', () => {
		render(CollectionFilterBar, {
			props: { series_options: seriesOptions, onfilterchange }
		});

		const select = screen.getByLabelText('Serie') as HTMLSelectElement;
		expect(select).toBeInTheDocument();
		expect(select.options).toHaveLength(3); // "Alle Serien" + 2 series
		expect(select.options[0]).toHaveTextContent('Alle Serien');
		expect(select.options[1]).toHaveTextContent('Maddrax');
		expect(select.options[2]).toHaveTextContent('Perry Rhodan');
	});

	it('fires onfilterchange when series is selected', async () => {
		render(CollectionFilterBar, {
			props: { series_options: seriesOptions, onfilterchange }
		});

		const select = screen.getByLabelText('Serie') as HTMLSelectElement;
		await fireEvent.change(select, { target: { value: 'maddrax' } });

		expect(onfilterchange).toHaveBeenCalledWith(
			expect.objectContaining({ series_slug: 'maddrax', page: 1 })
		);
	});

	it('renders all status filter chips', () => {
		render(CollectionFilterBar, {
			props: { series_options: seriesOptions, onfilterchange }
		});

		expect(screen.getByTestId('status-filter-all')).toBeInTheDocument();
		expect(screen.getByTestId('status-filter-owned')).toBeInTheDocument();
		expect(screen.getByTestId('status-filter-duplicate')).toBeInTheDocument();
		expect(screen.getByTestId('status-filter-wanted')).toBeInTheDocument();
		expect(screen.getByTestId('status-filter-missing')).toBeInTheDocument();
	});

	it('defaults to "Alle" status', () => {
		render(CollectionFilterBar, {
			props: { series_options: seriesOptions, onfilterchange }
		});

		expect(screen.getByTestId('status-filter-all')).toHaveAttribute('aria-checked', 'true');
		expect(screen.getByTestId('status-filter-owned')).toHaveAttribute('aria-checked', 'false');
	});

	it('fires onfilterchange when status chip is clicked', async () => {
		render(CollectionFilterBar, {
			props: { series_options: seriesOptions, onfilterchange }
		});
		const user = userEvent.setup();

		await user.click(screen.getByTestId('status-filter-owned'));

		expect(onfilterchange).toHaveBeenCalledWith(
			expect.objectContaining({ status: 'owned', page: 1 })
		);
	});

	it('updates aria-checked when status chip is clicked', async () => {
		render(CollectionFilterBar, {
			props: { series_options: seriesOptions, onfilterchange }
		});
		const user = userEvent.setup();

		await user.click(screen.getByTestId('status-filter-wanted'));

		expect(screen.getByTestId('status-filter-wanted')).toHaveAttribute('aria-checked', 'true');
		expect(screen.getByTestId('status-filter-all')).toHaveAttribute('aria-checked', 'false');
	});

	it('renders sort dropdown', () => {
		render(CollectionFilterBar, {
			props: { series_options: seriesOptions, onfilterchange }
		});

		const sortSelect = screen.getByLabelText('Sortierung') as HTMLSelectElement;
		expect(sortSelect).toBeInTheDocument();
		expect(sortSelect.options).toHaveLength(4);
	});

	it('fires onfilterchange when sort changes', async () => {
		render(CollectionFilterBar, {
			props: { series_options: seriesOptions, onfilterchange }
		});

		const sortSelect = screen.getByLabelText('Sortierung');
		await fireEvent.change(sortSelect, { target: { value: 'title' } });

		expect(onfilterchange).toHaveBeenCalledWith(
			expect.objectContaining({ sort: 'title', page: 1 })
		);
	});

	it('renders sort direction toggle defaulting to ascending', () => {
		render(CollectionFilterBar, {
			props: { series_options: seriesOptions, onfilterchange }
		});

		const toggle = screen.getByTestId('sort-dir-toggle');
		expect(toggle).toBeInTheDocument();
		expect(toggle).toHaveTextContent('↑');
	});

	it('toggles sort direction on click', async () => {
		render(CollectionFilterBar, {
			props: { series_options: seriesOptions, onfilterchange }
		});
		const user = userEvent.setup();

		const toggle = screen.getByTestId('sort-dir-toggle');
		await user.click(toggle);

		expect(toggle).toHaveTextContent('↓');
		expect(onfilterchange).toHaveBeenCalledWith(
			expect.objectContaining({ sort_dir: 'desc', page: 1 })
		);
	});

	it('renders search input', () => {
		render(CollectionFilterBar, {
			props: { series_options: seriesOptions, onfilterchange }
		});

		const search = screen.getByTestId('search-input') as HTMLInputElement;
		expect(search).toBeInTheDocument();
		expect(search.type).toBe('search');
		expect(search.placeholder).toBe('Suchen...');
	});

	it('fires onfilterchange on search input (debounced)', async () => {
		vi.useFakeTimers();
		render(CollectionFilterBar, {
			props: { series_options: seriesOptions, onfilterchange }
		});

		const search = screen.getByTestId('search-input');
		await fireEvent.input(search, { target: { value: 'Dunkle' } });

		// Should not fire immediately (debounced)
		expect(onfilterchange).not.toHaveBeenCalled();

		// Advance past debounce delay
		vi.advanceTimersByTime(300);

		expect(onfilterchange).toHaveBeenCalledTimes(1);
		const call = (onfilterchange as ReturnType<typeof vi.fn>).mock.calls[0][0];
		expect(call.q).toBe('Dunkle');
		vi.useRealTimers();
	});

	it('does not include default values in params', async () => {
		render(CollectionFilterBar, {
			props: { series_options: seriesOptions, onfilterchange }
		});
		const user = userEvent.setup();

		// Click "Alle" which is already selected — emitChange still fires
		await user.click(screen.getByTestId('status-filter-all'));

		// Default params: no series_slug, no status, no sort (default=issue_number), no sort_dir (default=asc)
		expect(onfilterchange).toHaveBeenCalledWith({ page: 1 });
	});

	it('renders with empty series_options', () => {
		render(CollectionFilterBar, {
			props: { series_options: [], onfilterchange }
		});

		const select = screen.getByLabelText('Serie') as HTMLSelectElement;
		expect(select.options).toHaveLength(1); // Only "Alle Serien"
	});

	it('has accessible labels for all controls', () => {
		render(CollectionFilterBar, {
			props: { series_options: seriesOptions, onfilterchange }
		});

		expect(screen.getByLabelText('Serie')).toBeInTheDocument();
		expect(screen.getByLabelText('Status-Filter')).toBeInTheDocument();
		expect(screen.getByLabelText('Sortierung')).toBeInTheDocument();
		expect(screen.getByLabelText(/Aufsteigend/)).toBeInTheDocument();
		expect(screen.getByLabelText('Suche')).toBeInTheDocument();
	});

	it('disables "Fehlend" chip when no series is selected', () => {
		render(CollectionFilterBar, {
			props: { series_options: seriesOptions, onfilterchange }
		});

		const missingChip = screen.getByTestId('status-filter-missing');
		expect(missingChip).toHaveAttribute('aria-disabled', 'true');
	});

	it('enables "Fehlend" chip when a series is selected', async () => {
		render(CollectionFilterBar, {
			props: { series_options: seriesOptions, onfilterchange }
		});

		const select = screen.getByLabelText('Serie') as HTMLSelectElement;
		await fireEvent.change(select, { target: { value: 'maddrax' } });

		const missingChip = screen.getByTestId('status-filter-missing');
		expect(missingChip).toHaveAttribute('aria-disabled', 'false');
	});

	it('does not fire onfilterchange when disabled "Fehlend" chip is clicked', async () => {
		render(CollectionFilterBar, {
			props: { series_options: seriesOptions, onfilterchange }
		});
		const user = userEvent.setup();

		await user.click(screen.getByTestId('status-filter-missing'));

		// Should not have been called because no series is selected
		expect(onfilterchange).not.toHaveBeenCalled();
	});

	it('resets status to "Alle" when series is deselected while "Fehlend" is active', async () => {
		render(CollectionFilterBar, {
			props: { series_options: seriesOptions, onfilterchange }
		});

		const select = screen.getByLabelText('Serie') as HTMLSelectElement;
		// Select a series first
		await fireEvent.change(select, { target: { value: 'maddrax' } });
		const user = userEvent.setup();

		// Click Fehlend
		await user.click(screen.getByTestId('status-filter-missing'));

		// Deselect series
		await fireEvent.change(select, { target: { value: '' } });

		// "Alle" should be active again
		expect(screen.getByTestId('status-filter-all')).toHaveAttribute('aria-checked', 'true');
		expect(screen.getByTestId('status-filter-missing')).toHaveAttribute('aria-checked', 'false');
	});
});
