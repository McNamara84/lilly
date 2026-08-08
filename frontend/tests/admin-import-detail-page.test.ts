import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/svelte';
import { userEvent } from '@testing-library/user-event';
import ImportDetailPage from '../src/routes/admin/import/[id]/+page.svelte';

function createMockStore<T>(initial: T) {
	let value = initial;
	const subs = new Set<(v: T) => void>();
	return {
		subscribe(fn: (v: T) => void) {
			subs.add(fn);
			fn(value);
			return () => subs.delete(fn);
		},
		set(v: T) {
			value = v;
			subs.forEach((fn) => fn(v));
		}
	};
}

const mockPage = createMockStore({ params: { id: '5' } });

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
	trigger_type: 'manual' as const,
	scheduled_for: null,
	status: 'completed',
	total_issues: 100,
	imported_issues: 100,
	failed_issues: 0,
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
		part_number: 1,
		part_total: 2,
		cycle: 'Erster Zyklus',
		cover_artists: ['Koveck'],
		keywords: ['Sci-Fi'],
		notes: [],
		cover_url: null,
		cover_local_path: '/media/maddrax/001.jpg',
		source_wiki_url: 'https://example.test/mx1'
	},
	{
		id: 2,
		series_id: 1,
		issue_number: 2,
		title: 'Die Flucht',
		authors: [],
		published_at: null,
		part_number: null,
		part_total: null,
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
		expect(screen.getByTestId('progress-count')).toHaveTextContent('100 / 100 bearbeitet');
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

	it('renders zero progress when the import has no issues yet', async () => {
		vi.mocked(fetchImportJob).mockResolvedValue({
			...runningJob,
			status: 'pending',
			total_issues: 0,
			imported_issues: 0
		});

		render(ImportDetailPage);

		await waitFor(() => {
			expect(screen.getByTestId('progress-bar')).toBeInTheDocument();
		});

		const progressFill = screen.getByTestId('progress-bar').firstElementChild;
		expect(progressFill).toHaveStyle({ width: '0%' });
	});

	it('polls a running import and stops when it completes', async () => {
		vi.mocked(fetchImportJob).mockResolvedValueOnce(runningJob).mockResolvedValueOnce(completedJob);
		vi.mocked(fetchImportSeriesIssues).mockResolvedValue({
			data: [],
			page: 1,
			per_page: 50,
			total: 0
		});

		render(ImportDetailPage);

		await waitFor(() => {
			expect(fetchImportJob).toHaveBeenCalledTimes(1);
		});

		await vi.advanceTimersByTimeAsync(3000);

		await waitFor(() => {
			expect(fetchImportJob).toHaveBeenCalledTimes(2);
		});

		await vi.advanceTimersByTimeAsync(3000);
		expect(fetchImportJob).toHaveBeenCalledTimes(2);
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

	it('shows an issue loading error returned as an Error object', async () => {
		vi.mocked(fetchImportJob).mockResolvedValue(completedJob);
		vi.mocked(fetchImportSeriesIssues).mockRejectedValue(new Error('Issue fetch failed'));

		render(ImportDetailPage);

		await waitFor(() => {
			expect(screen.getByTestId('error-message')).toHaveTextContent('Issue fetch failed');
		});
	});

	it('shows a generic issue loading error for a non-Error rejection', async () => {
		vi.mocked(fetchImportJob).mockResolvedValue(completedJob);
		vi.mocked(fetchImportSeriesIssues).mockRejectedValue('unexpected');

		render(ImportDetailPage);

		await waitFor(() => {
			expect(screen.getByTestId('error-message')).toHaveTextContent('Failed to load issues');
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

	it('shows a generic activation error for a non-Error rejection', async () => {
		vi.mocked(fetchImportJob).mockResolvedValue(completedJob);
		vi.mocked(fetchImportSeriesIssues).mockResolvedValue({
			data: [],
			page: 1,
			per_page: 50,
			total: 0
		});
		vi.mocked(activateSeries).mockRejectedValue('unexpected');

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

	it('shows cover image, multipart position and source link', async () => {
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

		expect(screen.getByAltText('Cover von #1: Der Gläserne Sarg')).toBeInTheDocument();
		expect(screen.getByText('1 von 2')).toBeInTheDocument();
		expect(screen.getByRole('link', { name: 'Quelle' })).toHaveAttribute(
			'href',
			'https://example.test/mx1'
		);
	});

	it('uses the remote cover when no local cover is available', async () => {
		vi.mocked(fetchImportJob).mockResolvedValue(completedJob);
		vi.mocked(fetchImportSeriesIssues).mockResolvedValue({
			data: [
				{
					...mockIssues[0],
					cover_local_path: null,
					cover_url: 'https://example.test/remote-cover.jpg'
				}
			],
			page: 1,
			per_page: 50,
			total: 1
		});

		render(ImportDetailPage);

		await waitFor(() => {
			expect(screen.getByAltText('Cover von #1: Der Gläserne Sarg')).toHaveAttribute(
				'src',
				'https://example.test/remote-cover.jpg'
			);
		});
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
			expect(screen.getByText(/Adapter: maddrax/)).toBeInTheDocument();
		});
	});

	it('labels scheduled imports as automatic runs', async () => {
		vi.mocked(fetchImportJob).mockResolvedValue({
			...completedJob,
			trigger_type: 'scheduled',
			scheduled_for: '2026-08-08T04:10:00Z',
			started_by: null
		});
		vi.mocked(fetchImportSeriesIssues).mockResolvedValue({
			data: [],
			page: 1,
			per_page: 50,
			total: 0
		});

		render(ImportDetailPage);

		await waitFor(() => {
			expect(screen.getByText(/Automatischer Lauf/)).toBeInTheDocument();
		});
	});

	it('treats completed_with_errors as terminal and displays partial results', async () => {
		vi.mocked(fetchImportJob).mockResolvedValue({
			...completedJob,
			status: 'completed_with_errors',
			imported_issues: 98,
			failed_issues: 2,
			error_message: '#7: parse error'
		});
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
		expect(screen.getByTestId('progress-count')).toHaveTextContent(
			'98 erfolgreich, 2 fehlgeschlagen'
		);
	});
});
