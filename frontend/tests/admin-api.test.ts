import { describe, it, expect, vi, beforeEach } from 'vitest';
import {
	fetchAllSeries,
	activateSeries,
	deactivateSeries,
	fetchAdapters,
	startImport,
	fetchImportJob,
	fetchImportIssues,
	fetchImportHistory
} from '../src/lib/api/admin';

const mockFetch = vi.fn();
global.fetch = mockFetch;

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

	describe('fetchImportIssues', () => {
		it('fetches paginated issues for import job', async () => {
			const data = { data: [], page: 1, per_page: 50, total: 0 };
			mockFetch.mockResolvedValue(jsonResponse(data));

			const result = await fetchImportIssues(5);

			expect(mockFetch).toHaveBeenCalledWith('/api/v1/admin/import/5/issues?page=1', {
				credentials: 'same-origin'
			});
			expect(result).toEqual(data);
		});

		it('passes page parameter', async () => {
			const data = { data: [], page: 3, per_page: 50, total: 100 };
			mockFetch.mockResolvedValue(jsonResponse(data));

			await fetchImportIssues(5, 3);

			expect(mockFetch).toHaveBeenCalledWith('/api/v1/admin/import/5/issues?page=3', {
				credentials: 'same-origin'
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
