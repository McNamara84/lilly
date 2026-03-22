import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/svelte';
import SeriesPage from '../src/routes/series/+page.svelte';

vi.mock('$lib/api/series', () => ({
	fetchSeries: vi.fn()
}));

vi.mock('$app/paths', () => ({
	resolve: vi.fn((path: string) => path)
}));

import { fetchSeries } from '$lib/api/series';

const mockSeries = [
	{
		id: 1,
		name: 'Maddrax',
		slug: 'maddrax',
		publisher: 'Bastei',
		genre: 'Science-Fiction',
		frequency: 'biweekly',
		total_issues: 620,
		status: 'running',
		active: true,
		source_url: null
	},
	{
		id: 2,
		name: 'Perry Rhodan',
		slug: 'perry-rhodan',
		publisher: 'Pabel-Moewig',
		genre: 'Science-Fiction',
		frequency: 'weekly',
		total_issues: 3300,
		status: 'running',
		active: true,
		source_url: null
	}
];

describe('Series Page', () => {
	beforeEach(() => {
		vi.clearAllMocks();
	});

	it('shows loading state initially', () => {
		vi.mocked(fetchSeries).mockReturnValue(new Promise(() => {}));
		render(SeriesPage);

		expect(screen.getByTestId('loading-indicator')).toHaveTextContent('Lade Serien...');
	});

	it('renders series cards after loading', async () => {
		vi.mocked(fetchSeries).mockResolvedValue(mockSeries);
		render(SeriesPage);

		await waitFor(() => {
			expect(screen.getByTestId('series-grid')).toBeInTheDocument();
		});

		const cards = screen.getAllByTestId('series-card');
		expect(cards).toHaveLength(2);
		expect(screen.getByText('Maddrax')).toBeInTheDocument();
		expect(screen.getByText('Perry Rhodan')).toBeInTheDocument();
	});

	it('shows empty state when no series available', async () => {
		vi.mocked(fetchSeries).mockResolvedValue([]);
		render(SeriesPage);

		await waitFor(() => {
			expect(screen.getByTestId('empty-state')).toBeInTheDocument();
		});
	});

	it('shows error on fetch failure', async () => {
		vi.mocked(fetchSeries).mockRejectedValue(new Error('Network error'));
		render(SeriesPage);

		await waitFor(() => {
			expect(screen.getByTestId('error-message')).toHaveTextContent('Network error');
		});
	});

	it('renders genre badges', async () => {
		vi.mocked(fetchSeries).mockResolvedValue(mockSeries);
		render(SeriesPage);

		await waitFor(() => {
			expect(screen.getByTestId('series-grid')).toBeInTheDocument();
		});

		const badges = screen.getAllByText('Science-Fiction');
		expect(badges).toHaveLength(2);
	});

	it('renders publisher info', async () => {
		vi.mocked(fetchSeries).mockResolvedValue(mockSeries);
		render(SeriesPage);

		await waitFor(() => {
			expect(screen.getByText('Bastei')).toBeInTheDocument();
			expect(screen.getByText('Pabel-Moewig')).toBeInTheDocument();
		});
	});

	it('has page title', () => {
		vi.mocked(fetchSeries).mockReturnValue(new Promise(() => {}));
		render(SeriesPage);

		expect(screen.getByTestId('series-title')).toHaveTextContent('Serien');
	});

	it('series cards are links with correct href', async () => {
		vi.mocked(fetchSeries).mockResolvedValue(mockSeries);
		render(SeriesPage);

		await waitFor(() => {
			expect(screen.getByTestId('series-grid')).toBeInTheDocument();
		});

		const cards = screen.getAllByTestId('series-card');
		expect(cards[0]).toHaveAttribute('href', '/series/maddrax');
		expect(cards[1]).toHaveAttribute('href', '/series/perry-rhodan');
	});
});
