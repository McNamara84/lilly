import { describe, it, expect, vi, beforeEach } from 'vitest';
import {
	fetchAllSeries,
	activateSeries,
	deactivateSeries,
	fetchAdapters,
	startImport,
	fetchImportJob,
	cancelImport,
	retryImport,
	fetchImportErrors,
	fetchImportSeriesIssues,
	fetchImportReviewItems,
	fetchImportReviewSummary,
	activateImport,
	AdminApiError,
	fetchImportHistory,
	fetchImportSchedule
} from '../src/lib/api/admin';

const mockFetch = vi.fn();
globalThis.fetch = mockFetch;

function jsonResponse(data: unknown, status = 200) {
	return {
		ok: status >= 200 && status < 300,
		status,
		json: () => Promise.resolve(data)
	};
}

describe('Admin API', () => {
	beforeEach(() => {
		vi.clearAllMocks();
	});

	describe('fetchAllSeries', () => {
		it('fetches all series from admin endpoint', async () => {
			const series = [{ id: 1, name: 'Maddrax', slug: 'maddrax', active: false }];
			mockFetch.mockResolvedValue(jsonResponse(series));

			const result = await fetchAllSeries();

			expect(mockFetch).toHaveBeenCalledWith('/api/v1/admin/series', {
				credentials: 'same-origin'
			});
			expect(result).toEqual(series);
		});

		it('throws on error response', async () => {
			mockFetch.mockResolvedValue(jsonResponse({ error: 'Unauthorized' }, 401));

			await expect(fetchAllSeries()).rejects.toThrow('Unauthorized');
		});
	});

	describe('activateSeries', () => {
		it('sends POST to activate endpoint', async () => {
			mockFetch.mockResolvedValue(jsonResponse({ message: 'Series activated' }));

			await activateSeries('maddrax');

			expect(mockFetch).toHaveBeenCalledWith('/api/v1/admin/series/maddrax/activate', {
				method: 'POST',
				credentials: 'same-origin'
			});
		});

		it('encodes slug with special characters', async () => {
			mockFetch.mockResolvedValue(jsonResponse({ message: 'ok' }));

			await activateSeries('my series');

			expect(mockFetch).toHaveBeenCalledWith(
				'/api/v1/admin/series/my%20series/activate',
				expect.objectContaining({ method: 'POST' })
			);
		});
	});

	describe('deactivateSeries', () => {
		it('sends POST to deactivate endpoint', async () => {
			mockFetch.mockResolvedValue(jsonResponse({ message: 'Series deactivated' }));

			await deactivateSeries('maddrax');

			expect(mockFetch).toHaveBeenCalledWith('/api/v1/admin/series/maddrax/deactivate', {
				method: 'POST',
				credentials: 'same-origin'
			});
		});
	});

	describe('fetchAdapters', () => {
		it('fetches adapter list', async () => {
			const adapters = [{ name: 'maddrax', display_name: 'Maddrax', version: '0.9' }];
			mockFetch.mockResolvedValue(jsonResponse(adapters));

			const result = await fetchAdapters();

			expect(mockFetch).toHaveBeenCalledWith('/api/v1/admin/adapters', {
				credentials: 'same-origin'
			});
			expect(result).toEqual(adapters);
		});
	});

	describe('startImport', () => {
		it('sends POST with adapter name', async () => {
			const job = { id: 1, adapter_name: 'maddrax', status: 'pending' };
			mockFetch.mockResolvedValue(jsonResponse(job));

			const result = await startImport('maddrax');

			expect(mockFetch).toHaveBeenCalledWith('/api/v1/admin/import', {
				method: 'POST',
				headers: { 'Content-Type': 'application/json' },
				credentials: 'same-origin',
				body: JSON.stringify({ adapter: 'maddrax' })
			});
			expect(result).toEqual(job);
		});

		it('throws on failed import start', async () => {
			mockFetch.mockResolvedValue(jsonResponse({ error: 'Unknown adapter' }, 400));

			await expect(startImport('unknown')).rejects.toThrow('Unknown adapter');
		});
	});

	describe('fetchImportJob', () => {
		it('fetches import job by id', async () => {
			const job = { id: 5, status: 'running', imported_issues: 10, total_issues: 100 };
			mockFetch.mockResolvedValue(jsonResponse(job));

			const result = await fetchImportJob(5);

			expect(mockFetch).toHaveBeenCalledWith('/api/v1/admin/import/5', {
				credentials: 'same-origin'
			});
			expect(result).toEqual(job);
		});

		it('throws on not found', async () => {
			mockFetch.mockResolvedValue(jsonResponse({ error: 'Not found' }, 404));

			await expect(fetchImportJob(999)).rejects.toThrow('Not found');
		});
	});

	describe('import lifecycle actions', () => {
		it('requests cancellation with POST', async () => {
			const job = { id: 5, status: 'running', cancel_requested_at: '2026-08-09T10:00:00' };
			mockFetch.mockResolvedValue(jsonResponse(job, 202));

			await expect(cancelImport(5)).resolves.toEqual(job);
			expect(mockFetch).toHaveBeenCalledWith('/api/v1/admin/import/5/cancel', {
				method: 'POST',
				credentials: 'same-origin'
			});
		});

		it('starts a linked retry with POST', async () => {
			const job = { id: 6, status: 'pending', retry_of_job_id: 5 };
			mockFetch.mockResolvedValue(jsonResponse(job, 202));

			await expect(retryImport(5)).resolves.toEqual(job);
			expect(mockFetch).toHaveBeenCalledWith('/api/v1/admin/import/5/retry', {
				method: 'POST',
				credentials: 'same-origin'
			});
		});

		it('loads structured import errors', async () => {
			const errors = {
				data: [{ id: 1, job_id: 5, issue_number: 7, stage: 'parse', message: 'bad row' }],
				page: 1,
				per_page: 50,
				total: 1
			};
			mockFetch.mockResolvedValue(jsonResponse(errors));

			await expect(fetchImportErrors(5)).resolves.toEqual(errors);
			expect(mockFetch).toHaveBeenCalledWith('/api/v1/admin/import/5/errors?page=1', {
				credentials: 'same-origin'
			});
		});
	});

	describe('fetchImportSeriesIssues', () => {
		it('fetches paginated series issues for import job', async () => {
			const data = { data: [], page: 1, per_page: 50, total: 0 };
			mockFetch.mockResolvedValue(jsonResponse(data));

			const result = await fetchImportSeriesIssues(5);

			expect(mockFetch).toHaveBeenCalledWith('/api/v1/admin/import/5/series-issues?page=1', {
				credentials: 'same-origin'
			});
			expect(result).toEqual(data);
		});

		it('passes page parameter', async () => {
			const data = { data: [], page: 3, per_page: 50, total: 100 };
			mockFetch.mockResolvedValue(jsonResponse(data));

			await fetchImportSeriesIssues(5, 3);

			expect(mockFetch).toHaveBeenCalledWith('/api/v1/admin/import/5/series-issues?page=3', {
				credentials: 'same-origin'
			});
		});
	});

	describe('import review', () => {
		it('loads the review summary for a concrete job', async () => {
			const summary = { job_id: 5, warning_count: 0, blocking_count: 0 };
			mockFetch.mockResolvedValue(jsonResponse(summary));

			await expect(fetchImportReviewSummary(5)).resolves.toEqual(summary);
			expect(mockFetch).toHaveBeenCalledWith('/api/v1/admin/import/5/review/summary', {
				credentials: 'same-origin'
			});
		});

		it('serializes pagination, search, filters and the pinned sample', async () => {
			const result = { items: [], page: 2, per_page: 25, total: 0 };
			mockFetch.mockResolvedValue(jsonResponse(result));

			await expect(
				fetchImportReviewItems(5, {
					page: 2,
					perPage: 25,
					query: '  Jason Dark  ',
					outcome: 'updated',
					severity: 'warning',
					coverStatus: 'fetch_failed',
					sample: true
				})
			).resolves.toEqual(result);
			expect(mockFetch).toHaveBeenCalledWith(
				'/api/v1/admin/import/5/review/items?page=2&per_page=25&q=Jason+Dark&outcome=updated&severity=warning&cover_status=fetch_failed&sample=true',
				{ credentials: 'same-origin' }
			);
		});

		it('activates only the selected job and sends warning acknowledgement explicitly', async () => {
			const result = { series_id: 1, active: true, event: null };
			mockFetch.mockResolvedValue(jsonResponse(result));

			await expect(activateImport(5, true)).resolves.toEqual(result);
			expect(mockFetch).toHaveBeenCalledWith('/api/v1/admin/import/5/activate', {
				method: 'POST',
				headers: { 'Content-Type': 'application/json' },
				credentials: 'same-origin',
				body: JSON.stringify({ acknowledge_warnings: true })
			});
		});

		it('preserves structured conflict status and code', async () => {
			mockFetch.mockResolvedValue(
				jsonResponse(
					{ error: 'Warnings must be acknowledged', code: 'warning_acknowledgement_required' },
					409
				)
			);

			const error = await activateImport(5, false).catch((caught) => caught);
			expect(error).toBeInstanceOf(AdminApiError);
			expect(error).toMatchObject({
				status: 409,
				code: 'warning_acknowledgement_required',
				message: 'Warnings must be acknowledged'
			});
		});
	});

	describe('fetchImportHistory', () => {
		it('fetches import history', async () => {
			const history = [{ id: 1 }, { id: 2 }];
			mockFetch.mockResolvedValue(jsonResponse(history));

			const result = await fetchImportHistory();

			expect(mockFetch).toHaveBeenCalledWith('/api/v1/admin/import/history', {
				credentials: 'same-origin'
			});
			expect(result).toEqual(history);
		});
	});

	describe('fetchImportSchedule', () => {
		it('fetches the automatic import schedule', async () => {
			const status = {
				enabled: true,
				schedule: '0 10 6 * * Sat *',
				timezone: 'Europe/Berlin',
				adapters: ['maddrax', 'john-sinclair'],
				next_run: '2026-08-08T04:10:00Z'
			};
			mockFetch.mockResolvedValue(jsonResponse(status));

			const result = await fetchImportSchedule();

			expect(mockFetch).toHaveBeenCalledWith('/api/v1/admin/import/schedule', {
				credentials: 'same-origin'
			});
			expect(result).toEqual(status);
		});
	});

	describe('error handling', () => {
		it('handles non-JSON error responses', async () => {
			mockFetch.mockResolvedValue({
				ok: false,
				status: 500,
				json: () => Promise.reject(new Error('not json'))
			});

			await expect(fetchAllSeries()).rejects.toThrow('An unexpected error occurred');
		});
	});
});
