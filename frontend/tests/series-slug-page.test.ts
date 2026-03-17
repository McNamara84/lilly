import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/svelte';
import { userEvent } from '@testing-library/user-event';
import { writable } from 'svelte/store';
import SeriesSlugPage from '../src/routes/series/[slug]/+page.svelte';

const mockPage = writable({ params: { slug: 'maddrax' } });

vi.mock('$app/stores', () => ({
	page: {
		subscribe: (fn: (value: unknown) => void) => mockPage.subscribe(fn)
	}
}));

vi.mock('$lib/api/series', () => ({
	fetchSeries: vi.fn(),
	fetchSeriesIssues: vi.fn()
}));

import { fetchSeries, fetchSeriesIssues } from '$lib/api/series';

const mockSeriesList = [
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
	}
];

const mockIssues = [
	{
		id: 1,
		series_id: 1,
		issue_number: 1,
		title: 'Der Gläserne Sarg',
		authors: ['Timothy Stahl'],
		published_at: '2000-02-08',
		cycle: 'Erster Zyklus',
		cover_artists: ['Koveck'],
		keywords: [],
		notes: [],
		cover_url: null,
		cover_local_path: '/media/maddrax/001.jpg',
		source_wiki_url: null
	},
	{
		id: 2,
		series_id: 1,
		issue_number: 2,
		title: 'Die Flucht',
		authors: [],
		published_at: null,
		cycle: null,
		cover_artists: [],
		keywords: [],
		notes: [],
		cover_url: null,
		cover_local_path: null,
		source_wiki_url: null
	}
];

describe('Series Slug Page', () => {
	beforeEach(() => {
		vi.clearAllMocks();
		mockPage.set({ params: { slug: 'maddrax' } });
	});

	it('shows loading state initially', () => {
		vi.mocked(fetchSeries).mockReturnValue(new Promise(() => {}));
		render(SeriesSlugPage);

		expect(screen.getByTestId('loading-indicator')).toHaveTextContent('Lade Serie...');
	});

	it('renders series header with name and metadata', async () => {
		vi.mocked(fetchSeries).mockResolvedValue(mockSeriesList);
		vi.mocked(fetchSeriesIssues).mockResolvedValue({
			data: mockIssues,
			page: 1,
			per_page: 50,
			total: 2
		});

		render(SeriesSlugPage);

		await waitFor(() => {
			expect(screen.getByTestId('series-header')).toBeInTheDocument();
		});

		expect(screen.getByText('Maddrax')).toBeInTheDocument();
		expect(screen.getByText(/Bastei/)).toBeInTheDocument();
		expect(screen.getByText(/Science-Fiction/)).toBeInTheDocument();
		expect(screen.getByText(/biweekly/)).toBeInTheDocument();
		expect(screen.getByText('620 Hefte')).toBeInTheDocument();
	});

	it('renders issue cards', async () => {
		vi.mocked(fetchSeries).mockResolvedValue(mockSeriesList);
		vi.mocked(fetchSeriesIssues).mockResolvedValue({
			data: mockIssues,
			page: 1,
			per_page: 50,
			total: 2
		});

		render(SeriesSlugPage);

		await waitFor(() => {
			expect(screen.getByTestId('issues-grid')).toBeInTheDocument();
		});

		const cards = screen.getAllByTestId('issue-card');
		expect(cards).toHaveLength(2);
		expect(screen.getByText('Nr. 1')).toBeInTheDocument();
		expect(screen.getByText('Der Gläserne Sarg')).toBeInTheDocument();
	});

	it('shows cover image for issues with cover', async () => {
		vi.mocked(fetchSeries).mockResolvedValue(mockSeriesList);
		vi.mocked(fetchSeriesIssues).mockResolvedValue({
			data: [mockIssues[0]],
			page: 1,
			per_page: 50,
			total: 1
		});

		render(SeriesSlugPage);

		await waitFor(() => {
			expect(screen.getByTestId('issues-grid')).toBeInTheDocument();
		});

		const img = screen.getByAltText('Cover von Der Gläserne Sarg');
		expect(img).toHaveAttribute('src', '/media/maddrax/001.jpg');
	});

	it('shows placeholder for issues without cover', async () => {
		vi.mocked(fetchSeries).mockResolvedValue(mockSeriesList);
		vi.mocked(fetchSeriesIssues).mockResolvedValue({
			data: [mockIssues[1]],
			page: 1,
			per_page: 50,
			total: 1
		});

		render(SeriesSlugPage);

		await waitFor(() => {
			expect(screen.getByTestId('issues-grid')).toBeInTheDocument();
		});

		expect(screen.getByLabelText('Kein Cover verfügbar')).toBeInTheDocument();
	});

	it('shows empty state when no issues', async () => {
		vi.mocked(fetchSeries).mockResolvedValue(mockSeriesList);
		vi.mocked(fetchSeriesIssues).mockResolvedValue({
			data: [],
			page: 1,
			per_page: 50,
			total: 0
		});

		render(SeriesSlugPage);

		await waitFor(() => {
			expect(screen.getByTestId('empty-issues')).toHaveTextContent('Noch keine Hefte verfügbar.');
		});
	});

	it('shows error when series not found', async () => {
		vi.mocked(fetchSeries).mockResolvedValue([]);

		render(SeriesSlugPage);

		await waitFor(() => {
			expect(screen.getByTestId('error-message')).toHaveTextContent('Serie nicht gefunden');
		});
	});

	it('shows error on fetch failure', async () => {
		vi.mocked(fetchSeries).mockRejectedValue(new Error('Network error'));

		render(SeriesSlugPage);

		await waitFor(() => {
			expect(screen.getByTestId('error-message')).toHaveTextContent('Network error');
		});
	});

	it('shows error when slug is empty', async () => {
		mockPage.set({ params: { slug: '' } });

		render(SeriesSlugPage);

		await waitFor(() => {
			expect(screen.getByTestId('error-message')).toHaveTextContent('Serie nicht gefunden');
		});

		expect(fetchSeries).not.toHaveBeenCalled();
	});

	it('renders pagination for multiple pages', async () => {
		vi.mocked(fetchSeries).mockResolvedValue(mockSeriesList);
		// 120 total issues = 3 pages at 50 per page
		vi.mocked(fetchSeriesIssues).mockResolvedValue({
			data: mockIssues,
			page: 1,
			per_page: 50,
			total: 120
		});

		render(SeriesSlugPage);

		await waitFor(() => {
			expect(screen.getByTestId('pagination')).toBeInTheDocument();
		});

		const pageButtons = screen.getAllByRole('button');
		expect(pageButtons).toHaveLength(3);
		expect(pageButtons[0]).toHaveAttribute('aria-current', 'page');
	});

	it('does not render pagination for single page', async () => {
		vi.mocked(fetchSeries).mockResolvedValue(mockSeriesList);
		vi.mocked(fetchSeriesIssues).mockResolvedValue({
			data: mockIssues,
			page: 1,
			per_page: 50,
			total: 2
		});

		render(SeriesSlugPage);

		await waitFor(() => {
			expect(screen.getByTestId('issues-grid')).toBeInTheDocument();
		});

		expect(screen.queryByTestId('pagination')).not.toBeInTheDocument();
	});

	it('calls fetchSeriesIssues on pagination click', async () => {
		vi.mocked(fetchSeries).mockResolvedValue(mockSeriesList);
		vi.mocked(fetchSeriesIssues).mockResolvedValue({
			data: mockIssues,
			page: 1,
			per_page: 50,
			total: 120
		});

		const user = userEvent.setup();
		render(SeriesSlugPage);

		await waitFor(() => {
			expect(screen.getByTestId('pagination')).toBeInTheDocument();
		});

		vi.mocked(fetchSeriesIssues).mockResolvedValue({
			data: mockIssues,
			page: 2,
			per_page: 50,
			total: 120
		});

		const page2Button = screen.getByRole('button', { name: '2' });
		await user.click(page2Button);

		expect(fetchSeriesIssues).toHaveBeenCalledWith('maddrax', 2);
	});

	it('handles series without optional metadata fields', async () => {
		const minimalSeries = [
			{
				...mockSeriesList[0],
				publisher: null,
				genre: null,
				frequency: null,
				total_issues: null
			}
		];
		vi.mocked(fetchSeries).mockResolvedValue(minimalSeries);
		vi.mocked(fetchSeriesIssues).mockResolvedValue({
			data: [],
			page: 1,
			per_page: 50,
			total: 0
		});

		render(SeriesSlugPage);

		await waitFor(() => {
			expect(screen.getByTestId('series-header')).toBeInTheDocument();
		});

		expect(screen.queryByText(/Verlag:/)).not.toBeInTheDocument();
		expect(screen.queryByText(/Genre:/)).not.toBeInTheDocument();
	});
});
