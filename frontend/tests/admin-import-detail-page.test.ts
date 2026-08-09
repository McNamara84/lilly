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

vi.mock('$app/navigation', () => ({
	goto: vi.fn()
}));

vi.mock('$lib/api/admin', () => ({
	fetchImportJob: vi.fn(),
	fetchImportSeriesIssues: vi.fn(),
	fetchImportReviewItems: vi.fn(),
	fetchImportReviewSummary: vi.fn(),
	fetchImportErrors: vi.fn(),
	cancelImport: vi.fn(),
	retryImport: vi.fn(),
	activateImport: vi.fn()
}));

import {
	fetchImportJob,
	fetchImportSeriesIssues,
	fetchImportReviewItems,
	fetchImportReviewSummary,
	fetchImportErrors,
	cancelImport,
	retryImport,
	activateImport
} from '$lib/api/admin';
import { goto } from '$app/navigation';

const completedJob = {
	id: 5,
	series_id: 1,
	series_slug: 'maddrax',
	adapter_name: 'maddrax',
	source_key: 'maddraxikon',
	trigger_type: 'manual' as const,
	scheduled_for: null,
	status: 'completed',
	total_issues: 100,
	imported_issues: 100,
	created_issues: 100,
	updated_issues: 0,
	unchanged_issues: 0,
	skipped_issues: 0,
	failed_issues: 0,
	error_message: null,
	started_by: 1,
	started_at: '2025-06-01T10:00:00Z',
	completed_at: '2025-06-01T10:15:00Z',
	created_at: '2025-06-01T10:00:00Z',
	updated_at: '2025-06-01T10:15:00Z',
	cancel_requested_at: null,
	retry_of_job_id: null
};

const runningJob = {
	...completedJob,
	status: 'running',
	imported_issues: 50,
	created_issues: 50,
	completed_at: null
};

const failedJob = {
	...completedJob,
	status: 'failed',
	imported_issues: 30,
	created_issues: 30,
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

const reviewSummary = {
	job_id: 5,
	series_id: 1,
	series_name: 'Maddrax',
	series_slug: 'maddrax',
	series_active: false,
	job_status: 'completed',
	outcomes: {
		total: 2,
		not_processed: 0,
		created: 2,
		updated: 0,
		unchanged: 0,
		skipped: 0,
		failed: 0
	},
	warning_count: 0,
	blocking_count: 0,
	eligibility: { eligible: true, requires_acknowledgement: false, reasons: [] },
	reference_checks: [
		{
			issue_number: 1,
			expected_title: 'Der Gläserne Sarg',
			expected_authors: ['Timothy Stahl'],
			expected_published_at: '2000-02-08',
			status: 'passed' as const,
			message: null
		}
	],
	sample_issue_numbers: [1, 2],
	last_publication_event: null
};

function asReviewItem(issue: (typeof mockIssues)[number]) {
	return {
		id: issue.id,
		job_id: 5,
		issue_id: issue.id,
		issue_number: issue.issue_number,
		outcome: 'created' as const,
		severity: 'info' as const,
		stage: 'complete',
		message: null,
		source_key: 'maddraxikon',
		source_record_id: `Quelle:MX${issue.issue_number}`,
		source_url: issue.source_wiki_url,
		title: issue.title,
		authors: issue.authors,
		cover_artists: issue.cover_artists,
		published_at: issue.published_at,
		part_number: issue.part_number,
		part_total: issue.part_total,
		cycle: issue.cycle,
		cover_status: (issue.cover_local_path || issue.cover_url ? 'imported' : 'missing_at_source') as
			'imported' | 'missing_at_source',
		cover_reason:
			issue.cover_local_path || issue.cover_url ? null : 'The source does not provide a cover',
		cover_local_path: issue.cover_local_path ?? issue.cover_url,
		processed_at: '2025-06-01T10:15:00Z'
	};
}

describe('Import Detail Page', () => {
	beforeEach(() => {
		vi.clearAllMocks();
		vi.useFakeTimers({ shouldAdvanceTime: true });
		mockPage.set({ params: { id: '5' } });
		vi.mocked(fetchImportErrors).mockResolvedValue({
			data: [],
			page: 1,
			per_page: 50,
			total: 0
		});
		vi.mocked(fetchImportSeriesIssues).mockResolvedValue({
			data: [],
			page: 1,
			per_page: 50,
			total: 0
		});
		vi.mocked(fetchImportReviewItems).mockImplementation(async (id, filters) => {
			const legacy = await fetchImportSeriesIssues(id, filters?.page ?? 1);
			return {
				items: legacy.data.map((issue) => asReviewItem(issue as (typeof mockIssues)[number])),
				page: legacy.page,
				per_page: legacy.per_page,
				total: legacy.total
			};
		});
		vi.mocked(fetchImportReviewSummary).mockResolvedValue(reviewSummary);
		vi.mocked(activateImport).mockResolvedValue({ series_id: 1, active: true, event: null });
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

		expect(screen.getByTestId('activate-series-button')).toHaveTextContent(
			'Geprüften Import freigeben'
		);
	});

	it('activates the reviewed import when the activation button is clicked', async () => {
		vi.mocked(fetchImportJob).mockResolvedValue(completedJob);
		vi.mocked(fetchImportSeriesIssues).mockResolvedValue({
			data: [],
			page: 1,
			per_page: 50,
			total: 0
		});
		const user = userEvent.setup({ advanceTimers: vi.advanceTimersByTime });
		render(ImportDetailPage);

		await waitFor(() => {
			expect(screen.getByTestId('activate-series-button')).toBeInTheDocument();
		});

		await user.click(screen.getByTestId('activate-series-button'));

		expect(activateImport).toHaveBeenCalledWith(5, false);
	});

	it('requires explicit acknowledgement before activating a warning-only import', async () => {
		vi.mocked(fetchImportJob).mockResolvedValue({
			...completedJob,
			status: 'completed_with_errors'
		});
		vi.mocked(fetchImportReviewSummary).mockResolvedValue({
			...reviewSummary,
			job_status: 'completed_with_errors',
			warning_count: 2,
			eligibility: { eligible: true, requires_acknowledgement: true, reasons: [] }
		});
		const user = userEvent.setup({ advanceTimers: vi.advanceTimersByTime });
		render(ImportDetailPage);

		await waitFor(() => expect(screen.getByTestId('activate-series-button')).toBeDisabled());
		await user.click(
			screen.getByRole('checkbox', {
				name: 'Ich habe alle Warnungen geprüft und gebe die Serie trotzdem frei.'
			})
		);
		expect(screen.getByTestId('activate-series-button')).toBeEnabled();
		await user.click(screen.getByTestId('activate-series-button'));

		expect(activateImport).toHaveBeenCalledWith(5, true);
		await waitFor(() => expect(screen.getByTestId('series-active-message')).toBeInTheDocument());
	});

	it('shows stable blocker reasons and prevents activation', async () => {
		vi.mocked(fetchImportJob).mockResolvedValue({
			...completedJob,
			status: 'completed_with_errors'
		});
		vi.mocked(fetchImportReviewSummary).mockResolvedValue({
			...reviewSummary,
			blocking_count: 1,
			eligibility: {
				eligible: false,
				requires_acknowledgement: false,
				reasons: [{ code: 'blocking_findings', message: 'The import contains blocking findings' }]
			}
		});
		render(ImportDetailPage);

		await waitFor(() => expect(screen.getByTestId('activation-blockers')).toBeInTheDocument());
		expect(screen.getByTestId('activation-blockers')).toHaveTextContent('blocking_findings');
		expect(screen.getByTestId('activate-series-button')).toBeDisabled();
		expect(activateImport).not.toHaveBeenCalled();
	});

	it('applies review search, risk and sample filters from the full result list', async () => {
		vi.mocked(fetchImportJob).mockResolvedValue(completedJob);
		const user = userEvent.setup({ advanceTimers: vi.advanceTimersByTime });
		render(ImportDetailPage);

		await waitFor(() => expect(screen.getByTestId('review-filters')).toBeInTheDocument());
		await user.type(screen.getByPlaceholderText('Nr., Titel, Autor, Quellen-ID'), 'Jason Dark');
		await user.selectOptions(screen.getByLabelText('Ergebnis'), 'updated');
		await user.selectOptions(screen.getByLabelText('Risiko'), 'warning');
		await user.selectOptions(screen.getByLabelText('Cover'), 'fetch_failed');
		await user.click(screen.getByRole('checkbox', { name: 'Nur Stichprobe' }));
		await user.click(screen.getByRole('button', { name: 'Anwenden' }));

		await waitFor(() => {
			expect(fetchImportReviewItems).toHaveBeenLastCalledWith(5, {
				page: 1,
				query: 'Jason Dark',
				outcome: 'updated',
				severity: 'warning',
				coverStatus: 'fetch_failed',
				sample: true
			});
		});
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
			expect(screen.getByTestId('error-message')).toHaveTextContent(
				'Failed to load review results'
			);
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
		expect(screen.getByTestId('sample-numbers')).toHaveTextContent(
			'Angeheftete Stichprobe: #1, #2'
		);
		expect(screen.getByTestId('reference-checks')).toHaveTextContent('Timothy Stahl · 2000-02-08');
	});

	it('shows error when activation fails', async () => {
		vi.mocked(fetchImportJob).mockResolvedValue(completedJob);
		vi.mocked(fetchImportSeriesIssues).mockResolvedValue({
			data: [],
			page: 1,
			per_page: 50,
			total: 0
		});
		vi.mocked(activateImport).mockRejectedValue(new Error('Activation failed'));

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
		vi.mocked(activateImport).mockRejectedValue('unexpected');

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
		const sourceLink = screen.getByRole('link', { name: 'Quelle' });
		expect(sourceLink).toHaveAttribute('href', 'https://example.test/mx1');
		expect(sourceLink).toHaveAttribute('target', '_blank');
		expect(sourceLink).toHaveAttribute('rel', 'noopener noreferrer');
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
			created_issues: 98,
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
			'98 neu, 0 geändert, 0 unverändert, 0 übersprungen, 2 fehlgeschlagen'
		);
	});

	it('requests cancellation for an active import', async () => {
		vi.mocked(fetchImportJob).mockResolvedValue(runningJob);
		vi.mocked(cancelImport).mockResolvedValue({
			...runningJob,
			cancel_requested_at: '2026-08-09T10:00:00Z'
		});
		const user = userEvent.setup({ advanceTimers: vi.advanceTimersByTime });
		render(ImportDetailPage);

		await waitFor(() => expect(screen.getByTestId('cancel-import-button')).toBeInTheDocument());
		await user.click(screen.getByTestId('cancel-import-button'));

		expect(cancelImport).toHaveBeenCalledWith(5);
		await waitFor(() => {
			expect(screen.getByTestId('cancel-import-button')).toHaveTextContent('Abbruch angefordert');
		});
	});

	it.each([
		[new Error('Cancellation rejected'), 'Cancellation rejected'],
		['unexpected', 'Cancellation failed']
	])('shows a cancellation error when the request fails', async (rejection, message) => {
		vi.mocked(fetchImportJob).mockResolvedValue(runningJob);
		vi.mocked(cancelImport).mockRejectedValue(rejection);
		const user = userEvent.setup({ advanceTimers: vi.advanceTimersByTime });
		render(ImportDetailPage);

		await waitFor(() => expect(screen.getByTestId('cancel-import-button')).toBeInTheDocument());
		await user.click(screen.getByTestId('cancel-import-button'));

		await waitFor(() => {
			expect(screen.getByTestId('error-message')).toHaveTextContent(message);
		});
	});

	it('starts a linked retry and navigates to the new job', async () => {
		vi.mocked(fetchImportJob).mockResolvedValue(failedJob);
		vi.mocked(retryImport).mockResolvedValue({
			...runningJob,
			id: 6,
			status: 'pending',
			retry_of_job_id: 5
		});
		const user = userEvent.setup({ advanceTimers: vi.advanceTimersByTime });
		render(ImportDetailPage);

		await waitFor(() => expect(screen.getByTestId('retry-import-button')).toBeInTheDocument());
		await user.click(screen.getByTestId('retry-import-button'));

		expect(retryImport).toHaveBeenCalledWith(5);
		expect(goto).toHaveBeenCalledWith('/admin/import/6');
		expect(screen.getByTestId('retry-import-button')).toBeEnabled();
	});

	it('resets persisted errors and their page when retry navigation reuses the route', async () => {
		const oldError = {
			id: 1,
			job_id: 5,
			source_key: 'maddraxikon',
			issue_number: 409,
			source_record_id: 'Quelle:MX409',
			stage: 'validate',
			message: 'old job error',
			created_at: '2026-08-09T10:00:00Z'
		};
		const newError = {
			...oldError,
			id: 2,
			job_id: 6,
			issue_number: 410,
			source_record_id: 'Quelle:MX410',
			message: 'new job error'
		};
		vi.mocked(fetchImportJob)
			.mockResolvedValueOnce(failedJob)
			.mockResolvedValueOnce({ ...failedJob, id: 6, retry_of_job_id: 5 });
		vi.mocked(fetchImportErrors)
			.mockResolvedValueOnce({ data: [oldError], page: 1, per_page: 50, total: 51 })
			.mockResolvedValueOnce({ data: [oldError], page: 2, per_page: 50, total: 51 })
			.mockResolvedValueOnce({ data: [newError], page: 1, per_page: 50, total: 1 });
		vi.mocked(retryImport).mockResolvedValue({
			...runningJob,
			id: 6,
			status: 'pending',
			retry_of_job_id: 5
		});
		vi.mocked(goto).mockImplementation(async () => {
			mockPage.set({ params: { id: '6' } });
		});
		const user = userEvent.setup({ advanceTimers: vi.advanceTimersByTime });
		render(ImportDetailPage);

		await waitFor(() =>
			expect(screen.getByTestId('job-errors-section')).toHaveTextContent('old job error')
		);
		await user.click(screen.getByTestId('next-errors-page'));
		await waitFor(() => expect(screen.getByText('Seite 2')).toBeInTheDocument());

		await user.click(screen.getByTestId('retry-import-button'));

		await waitFor(() => {
			expect(screen.getByTestId('import-title')).toHaveTextContent('Import #6');
			expect(screen.getByTestId('job-errors-section')).toHaveTextContent('new job error');
		});
		expect(screen.queryByText(/old job error/)).not.toBeInTheDocument();
		expect(fetchImportErrors).toHaveBeenLastCalledWith(6, 1);
		expect(screen.getByTestId('retry-import-button')).toBeEnabled();
	});

	it.each([
		[new Error('Retry rejected'), 'Retry rejected'],
		['unexpected', 'Retry failed']
	])('shows a retry error when the request fails', async (rejection, message) => {
		vi.mocked(fetchImportJob).mockResolvedValue(failedJob);
		vi.mocked(retryImport).mockRejectedValue(rejection);
		const user = userEvent.setup({ advanceTimers: vi.advanceTimersByTime });
		render(ImportDetailPage);

		await waitFor(() => expect(screen.getByTestId('retry-import-button')).toBeInTheDocument());
		await user.click(screen.getByTestId('retry-import-button'));

		await waitFor(() => {
			expect(screen.getByTestId('error-message')).toHaveTextContent(message);
		});
		expect(goto).not.toHaveBeenCalled();
	});

	it.each([
		[new Error('Error context unavailable'), 'Error context unavailable'],
		['unexpected', 'Failed to load import errors']
	])('shows an error when loading persisted error context fails', async (rejection, message) => {
		vi.mocked(fetchImportJob).mockResolvedValue(failedJob);
		vi.mocked(fetchImportErrors).mockRejectedValue(rejection);

		render(ImportDetailPage);

		await waitFor(() => {
			expect(screen.getByTestId('error-message')).toHaveTextContent(message);
		});
	});

	it('shows persisted issue-level error context', async () => {
		vi.mocked(fetchImportJob).mockResolvedValue(failedJob);
		vi.mocked(fetchImportErrors).mockResolvedValue({
			data: [
				{
					id: 1,
					job_id: 5,
					source_key: 'maddraxikon',
					issue_number: 409,
					source_record_id: 'Quelle:MX409',
					stage: 'validate',
					message: 'missing author',
					created_at: '2026-08-09T10:00:00Z'
				}
			],
			page: 1,
			per_page: 50,
			total: 1
		});
		render(ImportDetailPage);

		await waitFor(() => expect(screen.getByTestId('job-errors-section')).toBeInTheDocument());
		expect(screen.getByTestId('job-errors-section')).toHaveTextContent(
			'Heft #409 (maddraxikon:Quelle:MX409) [validate]: missing author'
		);
	});

	it('paginates persisted error context in both directions', async () => {
		const runError = {
			id: 1,
			job_id: 5,
			source_key: 'maddraxikon',
			issue_number: null,
			source_record_id: null,
			stage: 'fetch',
			message: 'wiki unavailable',
			created_at: '2026-08-09T10:00:00Z'
		};
		const issueError = {
			...runError,
			id: 51,
			issue_number: 695,
			message: 'invalid issue'
		};
		vi.mocked(fetchImportJob).mockResolvedValue(failedJob);
		vi.mocked(fetchImportErrors)
			.mockResolvedValueOnce({ data: [runError], page: 1, per_page: 50, total: 51 })
			.mockResolvedValueOnce({ data: [issueError], page: 2, per_page: 50, total: 51 })
			.mockResolvedValueOnce({ data: [runError], page: 1, per_page: 50, total: 51 });
		const user = userEvent.setup({ advanceTimers: vi.advanceTimersByTime });
		render(ImportDetailPage);

		await waitFor(() => expect(screen.getByTestId('job-errors-section')).toHaveTextContent('Lauf'));
		expect(screen.getByTestId('previous-errors-page')).toBeDisabled();
		expect(screen.getByTestId('next-errors-page')).toBeEnabled();

		await user.click(screen.getByTestId('next-errors-page'));
		await waitFor(() => {
			expect(fetchImportErrors).toHaveBeenLastCalledWith(5, 2);
			expect(screen.getByTestId('job-errors-section')).toHaveTextContent('Heft #695');
			expect(screen.getByText('Seite 2')).toBeInTheDocument();
		});
		expect(screen.getByTestId('previous-errors-page')).toBeEnabled();
		expect(screen.getByTestId('next-errors-page')).toBeDisabled();

		await user.click(screen.getByTestId('previous-errors-page'));
		await waitFor(() => {
			expect(fetchImportErrors).toHaveBeenLastCalledWith(5, 1);
			expect(screen.getByText('Seite 1')).toBeInTheDocument();
		});
	});

	it('shows source, retry origin and last update metadata', async () => {
		vi.mocked(fetchImportJob).mockResolvedValue({
			...failedJob,
			retry_of_job_id: 4
		});
		render(ImportDetailPage);

		await waitFor(() => expect(screen.getByTestId('import-title')).toBeInTheDocument());
		expect(screen.getByText(/Quelle: maddraxikon/)).toBeInTheDocument();
		expect(screen.getByText(/Wiederholung von #4/)).toBeInTheDocument();
		expect(screen.getByText(/Zuletzt aktualisiert:/)).toBeInTheDocument();
	});

	it.each(['cancelled', 'interrupted'])('treats %s as terminal', async (status) => {
		vi.mocked(fetchImportJob).mockResolvedValue({ ...failedJob, status });
		render(ImportDetailPage);

		await waitFor(() => expect(screen.getByTestId('job-status')).toHaveTextContent(status));
		expect(screen.getByTestId('retry-import-button')).toBeInTheDocument();
	});
});
