import { describe, it, expect, vi, beforeEach } from 'vitest';
import {
	fetchSeries,
	fetchSeriesIssues,
	fetchAllSeriesIssues,
	fetchIssue
} from '../src/lib/api/series';

const mockFetch = vi.fn();
globalThis.fetch = mockFetch;

function jsonResponse(data: unknown, status = 200) {
	return {
		ok: status >= 200 && status < 300,
		status,
		json: () => Promise.resolve(data)
	};
}

describe('Series API', () => {
	beforeEach(() => {
		vi.clearAllMocks();
	});

	describe('fetchSeries', () => {
		it('fetches active series list', async () => {
			const series = [{ id: 1, name: 'Maddrax', slug: 'maddrax', active: true, status: 'running' }];
			mockFetch.mockResolvedValue(jsonResponse(series));

			const result = await fetchSeries();

			expect(mockFetch).toHaveBeenCalledWith('/api/v1/series', {
				credentials: 'same-origin'
			});
			expect(result).toEqual(series);
		});

		it('throws on server error', async () => {
			mockFetch.mockResolvedValue(jsonResponse({ error: 'Server error' }, 500));

			await expect(fetchSeries()).rejects.toThrow('Server error');
		});

		it('handles empty series list', async () => {
			mockFetch.mockResolvedValue(jsonResponse([]));

			const result = await fetchSeries();
			expect(result).toEqual([]);
		});
	});

	describe('fetchSeriesIssues', () => {
		it('fetches paginated issues for a series', async () => {
			const data = {
				data: [{ id: 1, issue_number: 1, title: 'Dunkle Zukunft' }],
				page: 1,
				per_page: 50,
				total: 620
			};
			mockFetch.mockResolvedValue(jsonResponse(data));

			const result = await fetchSeriesIssues('maddrax');

			expect(mockFetch).toHaveBeenCalledWith('/api/v1/series/maddrax/issues?page=1', {
				credentials: 'same-origin'
			});
			expect(result.total).toBe(620);
		});

		it('passes custom page parameter', async () => {
			mockFetch.mockResolvedValue(jsonResponse({ data: [], page: 5, per_page: 50, total: 0 }));

			await fetchSeriesIssues('maddrax', 5);

			const url = mockFetch.mock.calls[0][0] as string;
			expect(url).toContain('page=5');
		});

		it('passes per_page parameter when provided', async () => {
			mockFetch.mockResolvedValue(jsonResponse({ data: [], page: 1, per_page: 100, total: 0 }));

			await fetchSeriesIssues('maddrax', 1, 100);

			const url = mockFetch.mock.calls[0][0] as string;
			expect(url).toContain('per_page=100');
		});

		it('encodes slug with special characters', async () => {
			mockFetch.mockResolvedValue(jsonResponse({ data: [], page: 1, per_page: 50, total: 0 }));

			await fetchSeriesIssues('my series');

			expect(mockFetch).toHaveBeenCalledWith(
				'/api/v1/series/my%20series/issues?page=1',
				expect.objectContaining({ credentials: 'same-origin' })
			);
		});

		it('throws on not found', async () => {
			mockFetch.mockResolvedValue(jsonResponse({ error: 'Not found' }, 404));

			await expect(fetchSeriesIssues('nonexistent')).rejects.toThrow('Not found');
		});
	});

	describe('fetchAllSeriesIssues', () => {
		it('fetches all pages until complete', async () => {
			const page1 = {
				data: Array.from({ length: 100 }, (_, i) => ({ id: i + 1, issue_number: i + 1 })),
				page: 1,
				per_page: 100,
				total: 150
			};
			const page2 = {
				data: Array.from({ length: 50 }, (_, i) => ({ id: i + 101, issue_number: i + 101 })),
				page: 2,
				per_page: 100,
				total: 150
			};
			mockFetch
				.mockResolvedValueOnce(jsonResponse(page1))
				.mockResolvedValueOnce(jsonResponse(page2));

			const result = await fetchAllSeriesIssues('maddrax');

			expect(result).toHaveLength(150);
			expect(mockFetch).toHaveBeenCalledTimes(2);
		});

		it('returns single page when total fits in one request', async () => {
			const data = {
				data: [{ id: 1, issue_number: 1 }],
				page: 1,
				per_page: 100,
				total: 1
			};
			mockFetch.mockResolvedValue(jsonResponse(data));

			const result = await fetchAllSeriesIssues('maddrax');

			expect(result).toHaveLength(1);
			expect(mockFetch).toHaveBeenCalledTimes(1);
		});

		it('handles empty series', async () => {
			mockFetch.mockResolvedValue(jsonResponse({ data: [], page: 1, per_page: 100, total: 0 }));

			const result = await fetchAllSeriesIssues('empty-series');

			expect(result).toHaveLength(0);
		});
	});

	describe('fetchIssue', () => {
		it('fetches a single issue by id', async () => {
			const issue = {
				id: 42,
				series_id: 1,
				issue_number: 42,
				title: 'Test Issue',
				authors: ['Author']
			};
			mockFetch.mockResolvedValue(jsonResponse(issue));

			const result = await fetchIssue(42);

			expect(mockFetch).toHaveBeenCalledWith('/api/v1/issues/42', {
				credentials: 'same-origin'
			});
			expect(result).toEqual(issue);
		});

		it('throws on not found', async () => {
			mockFetch.mockResolvedValue(jsonResponse({ error: 'Issue 999 not found' }, 404));

			await expect(fetchIssue(999)).rejects.toThrow('Issue 999 not found');
		});
	});

	describe('error handling', () => {
		it('handles non-JSON error response', async () => {
			mockFetch.mockResolvedValue({
				ok: false,
				status: 500,
				json: () => Promise.reject(new Error('not json'))
			});

			await expect(fetchSeries()).rejects.toThrow('An unexpected error occurred');
		});

		it('handles error response without error field', async () => {
			mockFetch.mockResolvedValue(jsonResponse({ message: 'something' }, 400));

			await expect(fetchSeries()).rejects.toThrow('An unexpected error occurred');
		});
	});
});
