import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, within } from '@testing-library/svelte';
import { userEvent } from '@testing-library/user-event';
import StatsCard from '../src/lib/components/stats/StatsCard.svelte';
import ConditionChips from '../src/lib/components/collection/ConditionChips.svelte';
import SeriesProgressBar from '../src/lib/components/collection/SeriesProgressBar.svelte';
import CoverCard from '../src/lib/components/collection/CoverCard.svelte';
import CoverGrid from '../src/lib/components/collection/CoverGrid.svelte';
import type { CollectionEntry } from '../src/lib/api/collection';

function makeEntry(overrides: Partial<CollectionEntry> = {}): CollectionEntry {
	return {
		id: 1,
		issue_id: 42,
		issue_number: 42,
		title: 'Dunkle Zukunft',
		series_id: 1,
		series_name: 'Maddrax',
		series_slug: 'maddrax',
		cover_url: null,
		cover_local_path: null,
		copy_number: 1,
		condition_grade: 'Z2',
		status: 'owned',
		notes: null,
		created_at: '2026-03-22T10:00:00Z',
		updated_at: '2026-03-22T10:00:00Z',
		...overrides
	};
}

// ---------------------------------------------------------------------------
// StatsCard
// ---------------------------------------------------------------------------
describe('StatsCard', () => {
	it('renders label and numeric value', () => {
		render(StatsCard, { props: { label: 'Gesamte Hefte', value: 42 } });

		expect(screen.getByTestId('stats-card')).toBeInTheDocument();
		expect(screen.getByText('Gesamte Hefte')).toBeInTheDocument();
		expect(screen.getByText('42')).toBeInTheDocument();
	});

	it('renders string value', () => {
		render(StatsCard, { props: { label: 'Fortschritt', value: '78.5%' } });

		expect(screen.getByText('78.5%')).toBeInTheDocument();
	});

	it('renders icon when provided', () => {
		render(StatsCard, { props: { label: 'Test', value: 0, icon: '📚' } });

		expect(screen.getByText('📚')).toBeInTheDocument();
	});

	it('hides icon from screen readers', () => {
		render(StatsCard, { props: { label: 'Test', value: 0, icon: '📚' } });

		const icon = screen.getByText('📚');
		expect(icon).toHaveAttribute('aria-hidden', 'true');
	});

	it('renders without icon', () => {
		render(StatsCard, { props: { label: 'Test', value: 0 } });

		expect(screen.getByTestId('stats-card')).toBeInTheDocument();
		expect(screen.queryByText('📚')).not.toBeInTheDocument();
	});
});

// ---------------------------------------------------------------------------
// ConditionChips
// ---------------------------------------------------------------------------
describe('ConditionChips', () => {
	let onchange: ReturnType<typeof vi.fn>;

	beforeEach(() => {
		onchange = vi.fn();
	});

	it('renders all 6 condition grades', () => {
		render(ConditionChips, { props: { value: null, onchange } });

		expect(screen.getByTestId('condition-chips')).toBeInTheDocument();
		for (const g of ['Z0', 'Z1', 'Z2', 'Z3', 'Z4', 'Z5']) {
			expect(screen.getByTestId(`condition-chip-${g}`)).toBeInTheDocument();
		}
	});

	it('marks selected chip as pressed', () => {
		render(ConditionChips, { props: { value: 'Z2', onchange } });

		expect(screen.getByTestId('condition-chip-Z2')).toHaveAttribute('aria-pressed', 'true');
		expect(screen.getByTestId('condition-chip-Z0')).toHaveAttribute('aria-pressed', 'false');
	});

	it('calls onchange when a chip is clicked', async () => {
		render(ConditionChips, { props: { value: 'Z2', onchange } });
		const user = userEvent.setup();

		await user.click(screen.getByTestId('condition-chip-Z4'));

		expect(onchange).toHaveBeenCalledWith('Z4');
	});

	it('disables chips when disabled prop is set', () => {
		render(ConditionChips, { props: { value: 'Z0', onchange, disabled: true } });

		expect(screen.getByTestId('condition-chip-Z0')).toBeDisabled();
		expect(screen.getByTestId('condition-chip-Z5')).toBeDisabled();
	});

	it('shows German sublabels', () => {
		render(ConditionChips, { props: { value: null, onchange } });

		expect(screen.getByText('Neuwertig')).toBeInTheDocument();
		expect(screen.getByText('Sehr gut')).toBeInTheDocument();
		expect(screen.getByText('Gut')).toBeInTheDocument();
		expect(screen.getByText('Akzeptabel')).toBeInTheDocument();
		expect(screen.getByText('Schlecht')).toBeInTheDocument();
		expect(screen.getByText('Sehr schlecht')).toBeInTheDocument();
	});

	it('has accessible fieldset label', () => {
		render(ConditionChips, { props: { value: null, onchange } });

		expect(screen.getByTestId('condition-chips')).toHaveAttribute(
			'aria-label',
			'Zustandsbewertung'
		);
	});
});

// ---------------------------------------------------------------------------
// SeriesProgressBar
// ---------------------------------------------------------------------------
describe('SeriesProgressBar', () => {
	it('renders series name and progress text', () => {
		render(SeriesProgressBar, {
			props: { series_name: 'Maddrax', owned_count: 300, total_count: 620 }
		});

		expect(screen.getByTestId('series-progress-bar')).toBeInTheDocument();
		expect(screen.getByText('Maddrax')).toBeInTheDocument();
		expect(screen.getByText(/300 von 620/)).toBeInTheDocument();
	});

	it('calculates percentage correctly', () => {
		render(SeriesProgressBar, {
			props: { series_name: 'Test', owned_count: 50, total_count: 200 }
		});

		expect(screen.getByText(/25\.0%/)).toBeInTheDocument();
	});

	it('handles zero total gracefully', () => {
		render(SeriesProgressBar, {
			props: { series_name: 'Empty', owned_count: 0, total_count: 0 }
		});

		expect(screen.getByText(/0\.0%/)).toBeInTheDocument();
	});

	it('renders progressbar with correct ARIA attributes', () => {
		render(SeriesProgressBar, {
			props: { series_name: 'Maddrax', owned_count: 310, total_count: 620 }
		});

		const bar = screen.getByRole('progressbar');
		expect(bar).toHaveAttribute('aria-valuemin', '0');
		expect(bar).toHaveAttribute('aria-valuemax', '100');
		expect(bar).toHaveAttribute('aria-label', 'Maddrax: 50.0% gesammelt');
	});

	it('shows duplicate count when present', () => {
		render(SeriesProgressBar, {
			props: { series_name: 'Test', owned_count: 10, total_count: 50, duplicate_count: 3 }
		});

		expect(screen.getByText('3 Doppelte')).toBeInTheDocument();
	});

	it('hides duplicate text when count is zero', () => {
		render(SeriesProgressBar, {
			props: { series_name: 'Test', owned_count: 10, total_count: 50, duplicate_count: 0 }
		});

		expect(screen.queryByText(/Doppelte/)).not.toBeInTheDocument();
	});
});

// ---------------------------------------------------------------------------
// CoverCard
// ---------------------------------------------------------------------------
describe('CoverCard', () => {
	it('renders issue number and title', () => {
		render(CoverCard, { props: { entry: makeEntry() } });

		const card = screen.getByTestId('cover-card');
		expect(card).toBeInTheDocument();
		expect(screen.getByText('Dunkle Zukunft')).toBeInTheDocument();
		expect(screen.getAllByText('#42').length).toBeGreaterThanOrEqual(1);
	});

	it('shows condition badge for non-missing entries', () => {
		render(CoverCard, { props: { entry: makeEntry({ condition_grade: 'Z1' }) } });

		expect(screen.getByText('Z1')).toBeInTheDocument();
	});

	it('hides condition badge for missing entries', () => {
		render(CoverCard, { props: { entry: makeEntry({ status: 'missing' }) } });

		expect(screen.queryByText('Z2')).not.toBeInTheDocument();
	});

	it('shows "Doppelt" label for duplicate status', () => {
		render(CoverCard, { props: { entry: makeEntry({ status: 'duplicate' }) } });

		expect(screen.getByText('Doppelt')).toBeInTheDocument();
	});

	it('renders cover image when cover_url is provided', () => {
		render(CoverCard, {
			props: {
				entry: makeEntry({ cover_url: 'https://example.com/cover.jpg' })
			}
		});

		const img = screen.getByAltText(/Cover von Maddrax #42/);
		expect(img).toHaveAttribute('src', 'https://example.com/cover.jpg');
	});

	it('prefers cover_local_path over cover_url', () => {
		render(CoverCard, {
			props: {
				entry: makeEntry({
					cover_url: 'https://example.com/cover.jpg',
					cover_local_path: '/local/cover.jpg'
				})
			}
		});

		const img = screen.getByAltText(/Cover von Maddrax #42/);
		expect(img).toHaveAttribute('src', '/local/cover.jpg');
	});

	it('shows placeholder when no cover is available', () => {
		render(CoverCard, { props: { entry: makeEntry() } });

		// The number appears multiple times; check it's present in the card
		const card = screen.getByTestId('cover-card');
		expect(within(card).getAllByText('#42').length).toBeGreaterThanOrEqual(1);
	});

	it('applies reduced opacity for missing entries', () => {
		render(CoverCard, { props: { entry: makeEntry({ status: 'missing' }) } });

		const card = screen.getByTestId('cover-card');
		expect(card.classList.contains('opacity-40')).toBe(true);
	});

	it('renders with button role when interactive', () => {
		render(CoverCard, { props: { entry: makeEntry(), interactive: true } });

		const card = screen.getByTestId('cover-card');
		expect(card).toHaveAttribute('role', 'button');
		expect(card).toHaveAttribute('tabindex', '0');
	});

	it('does not have button role when not interactive', () => {
		render(CoverCard, { props: { entry: makeEntry(), interactive: false } });

		const card = screen.getByTestId('cover-card');
		expect(card).not.toHaveAttribute('role');
	});

	it('calls onclick when clicked', async () => {
		const handleClick = vi.fn();
		render(CoverCard, { props: { entry: makeEntry(), onclick: handleClick } });
		const user = userEvent.setup();

		await user.click(screen.getByTestId('cover-card'));

		expect(handleClick).toHaveBeenCalledOnce();
	});

	it('has accessible aria-label', () => {
		render(CoverCard, {
			props: { entry: makeEntry({ title: 'Dunkle Zukunft', status: 'owned' }) }
		});

		const card = screen.getByTestId('cover-card');
		expect(card.getAttribute('aria-label')).toContain('Dunkle Zukunft');
		expect(card.getAttribute('aria-label')).toContain('#42');
	});

	it('uses medium size class by default', () => {
		render(CoverCard, { props: { entry: makeEntry() } });

		const card = screen.getByTestId('cover-card');
		expect(card.classList.contains('w-36')).toBe(true);
	});
});

// ---------------------------------------------------------------------------
// CoverGrid
// ---------------------------------------------------------------------------
describe('CoverGrid', () => {
	it('shows loading skeleton', () => {
		render(CoverGrid, { props: { items: [], loading: true } });

		expect(screen.getByTestId('cover-grid-skeleton')).toBeInTheDocument();
	});

	it('shows empty message when no items and not loading', () => {
		render(CoverGrid, { props: { items: [], loading: false } });

		expect(screen.getByTestId('cover-grid-empty')).toBeInTheDocument();
		expect(screen.getByText('Keine Hefte gefunden.')).toBeInTheDocument();
	});

	it('shows custom empty message', () => {
		render(CoverGrid, {
			props: { items: [], loading: false, empty_message: 'Sammlung leer' }
		});

		expect(screen.getByText('Sammlung leer')).toBeInTheDocument();
	});

	it('renders cover cards for items', () => {
		const items = [makeEntry({ id: 1, issue_number: 1 }), makeEntry({ id: 2, issue_number: 2 })];
		render(CoverGrid, { props: { items, loading: false } });

		expect(screen.getByTestId('cover-grid')).toBeInTheDocument();
		expect(screen.getAllByTestId('cover-card')).toHaveLength(2);
	});

	it('calls onselect when a card is clicked', async () => {
		const onselect = vi.fn();
		const entry = makeEntry();
		render(CoverGrid, { props: { items: [entry], loading: false, onselect } });
		const user = userEvent.setup();

		await user.click(screen.getByTestId('cover-card'));

		expect(onselect).toHaveBeenCalledWith(entry);
	});
});
