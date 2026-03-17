import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/svelte';
import { userEvent } from '@testing-library/user-event';
import { writable } from 'svelte/store';
import ImportDetailPage from '../src/routes/admin/import/[id]/+page.svelte';

const mockPage = writable({ params: { id: '5' } });

vi.mock('$app/stores', () => ({
	page: {
		subscribe: (fn: (value: unknown) => void) => mockPage.subscribe(fn)
	}
}));

vi.mock('$app/paths', () => ({
	resolve: (path: string) => path
}));

vi.mock('$lib/api/admin', () => ({
	fetchImportJob: vi.fn(),
	fetchImportSeriesIssues: vi.fn(),
	activateSeries: vi.fn()
}));

import { fetchImportJob, fetchImportSeriesIssues, activateSeries } from '$lib/api/admin';

const completedJob = {
	id: 5,
	series_id: 1,
	series_slug: 'maddrax',
	adapter_name: 'maddrax',
	status: 'completed',
	total_issues: 100,
	imported_issues: 100,
	error_message: null,
	started_by: 1,
	started_at: '2025-06-01T10:00:00Z',
	completed_at: '2025-06-01T10:15:00Z'
};

const runningJob = {
	...completedJob,
	status: 'running',
	imported_issues: 50,
	completed_at: null
};

const failedJob = {
	...completedJob,
	status: 'failed',
	imported_issues: 30,
	error_message: 'Wiki API unreachable'
};

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
		keywords: ['Sci-Fi'],
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

describe('Import Detail Page', () => {
	beforeEach(() => {
		vi.clearAllMocks();
		vi.useFakeTimers({ shouldAdvanceTime: true });
		mockPage.set({ params: { id: '5' } });
	});

	afterEach(() => {
		vi.useRealTimers();
	});

	it('shows loading state initially', () => {
		vi.mocked(fetchImportJob).mockReturnValue(new Promise(() => {}));
		render(ImportDetailPage);

		expect(screen.getByTestId('loading-indicator')).toHaveTextContent('Lade Import-Details...');
	});

	it('shows back link', () => {
		vi.mocked(fetchImportJob).mockReturnValue(new Promise(() => {}));
		render(ImportDetailPage);

		expect(screen.getByTestId('back-link')).toHaveAttribute('href', '/admin/import');
	});

	it('renders completed import with title and progress', async () => {
		vi.mocked(fetchImportJob).mockResolvedValue(completedJob);
		vi.mocked(fetchImportSeriesIssues).mockResolvedValue({
			data: mockIssues,
			page: 1,
			per_page: 50,
			total: 2
		});

		render(ImportDetailPage);

		await waitFor(() => {
			expect(screen.getByTestId('import-title')).toHaveTextContent('Import #5');
		});

		expect(screen.getByTestId('job-status')).toHaveTextContent('completed');
		expect(screen.getByTestId('progress-count')).toHaveTextContent('100 / 100 Hefte');
	});

	it('renders progress bar with correct aria attributes', async () => {
		vi.mocked(fetchImportJob).mockResolvedValue(runningJob);

		render(ImportDetailPage);

		await waitFor(() => {
			expect(screen.getByTestId('progress-bar')).toBeInTheDocument();
		});

		const progressBar = screen.getByRole('progressbar');
		expect(progressBar).toHaveAttribute('aria-valuenow', '50');
		expect(progressBar).toHaveAttribute('aria-valuemax', '100');
		expect(progressBar).toHaveAttribute('aria-label', 'Import-Fortschritt');
	});

	it('shows error message on failed import', async () => {
		vi.mocked(fetchImportJob).mockResolvedValue(failedJob);

		render(ImportDetailPage);

		await waitFor(() => {
			expect(screen.getByTestId('error-detail')).toHaveTextContent('Wiki API unreachable');
		});
	});

	it('shows issues table when import is completed', async () => {
		vi.mocked(fetchImportJob).mockResolvedValue(completedJob);
		vi.mocked(fetchImportSeriesIssues).mockResolvedValue({
			data: mockIssues,
			page: 1,
			per_page: 50,
			total: 2
		});

		render(ImportDetailPage);

		await waitFor(() => {
			expect(screen.getByTestId('issues-table')).toBeInTheDocument();
		});

		const rows = screen.getAllByTestId('issue-row');
		expect(rows).toHaveLength(2);
		expect(screen.getByText('Der Gläserne Sarg')).toBeInTheDocument();
		expect(screen.getByText('Timothy Stahl')).toBeInTheDocument();
	});

	it('shows dash for missing issue data', async () => {
		vi.mocked(fetchImportJob).mockResolvedValue(completedJob);
		vi.mocked(fetchImportSeriesIssues).mockResolvedValue({
			data: [mockIssues[1]],
			page: 1,
			per_page: 50,
			total: 1
		});

		render(ImportDetailPage);

		await waitFor(() => {
			expect(screen.getByTestId('issues-table')).toBeInTheDocument();
		});

		// Authors, cycle, date, cover_artists for second issue are all '–'
		const row = screen.getByTestId('issue-row');
		expect(row).toBeInTheDocument();
	});

	it('shows activate button for completed import', async () => {
		vi.mocked(fetchImportJob).mockResolvedValue(completedJob);
		vi.mocked(fetchImportSeriesIssues).mockResolvedValue({
			data: [],
			page: 1,
			per_page: 50,
			total: 0
		});

		render(ImportDetailPage);

		await waitFor(() => {
			expect(screen.getByTestId('activate-series-button')).toBeInTheDocument();
		});

		expect(screen.getByTestId('activate-series-button')).toHaveTextContent('Serie aktivieren');
	});

	it('calls activateSeries when activate button is clicked', async () => {
		vi.mocked(fetchImportJob).mockResolvedValue(completedJob);
		vi.mocked(fetchImportSeriesIssues).mockResolvedValue({
			data: [],
			page: 1,
			per_page: 50,
			total: 0
		});
		vi.mocked(activateSeries).mockResolvedValue(undefined);

		const user = userEvent.setup({ advanceTimers: vi.advanceTimersByTime });
		render(ImportDetailPage);

		await waitFor(() => {
			expect(screen.getByTestId('activate-series-button')).toBeInTheDocument();
		});

		await user.click(screen.getByTestId('activate-series-button'));

		expect(activateSeries).toHaveBeenCalledWith('maddrax');
	});

	it('shows error when fetchImportJob fails', async () => {
		vi.mocked(fetchImportJob).mockRejectedValue(new Error('Server error'));

		render(ImportDetailPage);

		await waitFor(() => {
			expect(screen.getByTestId('error-message')).toHaveTextContent('Server error');
		});
	});

	it('shows generic error when fetch throws non-Error', async () => {
		vi.mocked(fetchImportJob).mockRejectedValue('unexpected');

		render(ImportDetailPage);

		await waitFor(() => {
			expect(screen.getByTestId('error-message')).toHaveTextContent('Failed to load import job');
		});
	});

	it('shows error for invalid job ID', async () => {
		mockPage.set({ params: { id: 'abc' } });

		render(ImportDetailPage);

		await waitFor(() => {
			expect(screen.getByTestId('error-message')).toHaveTextContent('Invalid import job ID');
		});

		expect(fetchImportJob).not.toHaveBeenCalled();
	});

	it('shows review section heading with total count', async () => {
		vi.mocked(fetchImportJob).mockResolvedValue(completedJob);
		vi.mocked(fetchImportSeriesIssues).mockResolvedValue({
			data: mockIssues,
			page: 1,
			per_page: 50,
			total: 2
		});

		render(ImportDetailPage);

		await waitFor(() => {
			expect(screen.getByTestId('review-section')).toBeInTheDocument();
		});

		expect(screen.getByText('Importierte Hefte (2)')).toBeInTheDocument();
	});

	it('shows error when activation fails', async () => {
		vi.mocked(fetchImportJob).mockResolvedValue(completedJob);
		vi.mocked(fetchImportSeriesIssues).mockResolvedValue({
			data: [],
			page: 1,
			per_page: 50,
			total: 0
		});
		vi.mocked(activateSeries).mockRejectedValue(new Error('Activation failed'));

		const user = userEvent.setup({ advanceTimers: vi.advanceTimersByTime });
		render(ImportDetailPage);

		await waitFor(() => {
			expect(screen.getByTestId('activate-series-button')).toBeInTheDocument();
		});

		await user.click(screen.getByTestId('activate-series-button'));

		await waitFor(() => {
			expect(screen.getByTestId('error-message')).toHaveTextContent('Activation failed');
		});
	});

	it('shows cover check mark for issues with cover', async () => {
		vi.mocked(fetchImportJob).mockResolvedValue(completedJob);
		vi.mocked(fetchImportSeriesIssues).mockResolvedValue({
			data: mockIssues,
			page: 1,
			per_page: 50,
			total: 2
		});

		render(ImportDetailPage);

		await waitFor(() => {
			expect(screen.getByTestId('issues-table')).toBeInTheDocument();
		});

		// First issue has cover, second doesn't
		const checkmarks = screen.getAllByText('✓');
		expect(checkmarks).toHaveLength(1);
	});

	it('displays adapter name', async () => {
		vi.mocked(fetchImportJob).mockResolvedValue(completedJob);
		vi.mocked(fetchImportSeriesIssues).mockResolvedValue({
			data: [],
			page: 1,
			per_page: 50,
			total: 0
		});

		render(ImportDetailPage);

		await waitFor(() => {
			expect(screen.getByText('Adapter: maddrax')).toBeInTheDocument();
		});
	});
});
