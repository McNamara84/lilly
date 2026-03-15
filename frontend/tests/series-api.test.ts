import { describe, it, expect, vi, beforeEach } from 'vitest';
import { fetchSeries, fetchSeriesIssues, fetchIssue } from '../src/lib/api/series';

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

			expect(mockFetch).toHaveBeenCalledWith('/api/v1/series/maddrax/issues?page=5', {
				credentials: 'same-origin'
			});
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

	describe('fetchIssue', () => {
		it('fetches a single issue by id', async () => {
			const issue = {
				id: 42,
				series_id: 1,
				issue_number: 42,
				title: 'Test Issue',
				author: 'Author'
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
