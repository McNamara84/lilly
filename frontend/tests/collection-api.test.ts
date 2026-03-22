import { describe, it, expect, vi, beforeEach } from 'vitest';
import {
	fetchCollection,
	addToCollection,
	updateCollectionEntry,
	deleteCollectionEntry,
	fetchCollectionStats
} from '../src/lib/api/collection';

const mockFetch = vi.fn();
globalThis.fetch = mockFetch;

function jsonResponse(data: unknown, status = 200) {
	return {
		ok: status >= 200 && status < 300,
		status,
		json: () => Promise.resolve(data)
	};
}

const sampleEntry = {
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
	updated_at: '2026-03-22T10:00:00Z'
};

describe('Collection API', () => {
	beforeEach(() => {
		vi.clearAllMocks();
	});

	describe('fetchCollection', () => {
		it('fetches collection with default (empty) params', async () => {
			const data = { data: [sampleEntry], page: 1, per_page: 20, total: 1 };
			mockFetch.mockResolvedValue(jsonResponse(data));

			const result = await fetchCollection();

			expect(mockFetch).toHaveBeenCalledWith('/api/v1/me/collection', {
				credentials: 'same-origin'
			});
			expect(result).toEqual(data);
		});

		it('appends series_slug and status as query params', async () => {
			const data = { data: [], page: 1, per_page: 20, total: 0 };
			mockFetch.mockResolvedValue(jsonResponse(data));

			await fetchCollection({ series_slug: 'maddrax', status: 'owned' });

			const url = mockFetch.mock.calls[0][0] as string;
			expect(url).toContain('series_slug=maddrax');
			expect(url).toContain('status=owned');
		});

		it('appends page and per_page as query params', async () => {
			const data = { data: [], page: 3, per_page: 50, total: 200 };
			mockFetch.mockResolvedValue(jsonResponse(data));

			await fetchCollection({ page: 3, per_page: 50 });

			const url = mockFetch.mock.calls[0][0] as string;
			expect(url).toContain('page=3');
			expect(url).toContain('per_page=50');
		});

		it('appends sort, sort_dir and search query', async () => {
			const data = { data: [], page: 1, per_page: 20, total: 0 };
			mockFetch.mockResolvedValue(jsonResponse(data));

			await fetchCollection({ sort: 'title', sort_dir: 'desc', q: 'dark' });

			const url = mockFetch.mock.calls[0][0] as string;
			expect(url).toContain('sort=title');
			expect(url).toContain('sort_dir=desc');
			expect(url).toContain('q=dark');
		});

		it('omits undefined and empty string params', async () => {
			const data = { data: [], page: 1, per_page: 20, total: 0 };
			mockFetch.mockResolvedValue(jsonResponse(data));

			await fetchCollection({ series_slug: undefined, status: '', page: 2 });

			const url = mockFetch.mock.calls[0][0] as string;
			expect(url).not.toContain('series_slug');
			expect(url).not.toContain('status');
			expect(url).toContain('page=2');
		});

		it('appends condition_min and condition_max params', async () => {
			const data = { data: [], page: 1, per_page: 20, total: 0 };
			mockFetch.mockResolvedValue(jsonResponse(data));

			await fetchCollection({ condition_min: 'Z1', condition_max: 'Z3' });

			const url = mockFetch.mock.calls[0][0] as string;
			expect(url).toContain('condition_min=Z1');
			expect(url).toContain('condition_max=Z3');
		});

		it('throws on server error', async () => {
			mockFetch.mockResolvedValue(jsonResponse({ error: 'Unauthorized' }, 401));

			await expect(fetchCollection()).rejects.toThrow('Unauthorized');
		});

		it('handles empty collection', async () => {
			const data = { data: [], page: 1, per_page: 20, total: 0 };
			mockFetch.mockResolvedValue(jsonResponse(data));

			const result = await fetchCollection();
			expect(result.data).toEqual([]);
			expect(result.total).toBe(0);
		});
	});

	describe('addToCollection', () => {
		it('posts a new collection entry', async () => {
			mockFetch.mockResolvedValue(jsonResponse(sampleEntry, 201));

			const result = await addToCollection({
				issue_id: 42,
				condition_grade: 'Z2',
				status: 'owned'
			});

			expect(mockFetch).toHaveBeenCalledWith('/api/v1/me/collection', {
				method: 'POST',
				headers: { 'Content-Type': 'application/json' },
				credentials: 'same-origin',
				body: JSON.stringify({ issue_id: 42, condition_grade: 'Z2', status: 'owned' })
			});
			expect(result).toEqual(sampleEntry);
		});

		it('sends optional fields when provided', async () => {
			mockFetch.mockResolvedValue(jsonResponse(sampleEntry, 201));

			await addToCollection({
				issue_id: 42,
				condition_grade: 'Z0',
				notes: 'First edition',
				copy_number: 2
			});

			const body = JSON.parse(mockFetch.mock.calls[0][1].body);
			expect(body.notes).toBe('First edition');
			expect(body.copy_number).toBe(2);
		});

		it('throws on validation error', async () => {
			mockFetch.mockResolvedValue(jsonResponse({ error: 'Invalid condition_grade' }, 422));

			await expect(addToCollection({ issue_id: 42, condition_grade: 'invalid' })).rejects.toThrow(
				'Invalid condition_grade'
			);
		});
	});

	describe('updateCollectionEntry', () => {
		it('patches an existing entry with partial data', async () => {
			const updated = { ...sampleEntry, condition_grade: 'Z0' };
			mockFetch.mockResolvedValue(jsonResponse(updated));

			const result = await updateCollectionEntry(1, { condition_grade: 'Z0' });

			expect(mockFetch).toHaveBeenCalledWith('/api/v1/me/collection/1', {
				method: 'PATCH',
				headers: { 'Content-Type': 'application/json' },
				credentials: 'same-origin',
				body: JSON.stringify({ condition_grade: 'Z0' })
			});
			expect(result.condition_grade).toBe('Z0');
		});

		it('updates status and notes together', async () => {
			const updated = { ...sampleEntry, status: 'wanted', notes: 'Looking for this' };
			mockFetch.mockResolvedValue(jsonResponse(updated));

			const result = await updateCollectionEntry(1, {
				status: 'wanted',
				notes: 'Looking for this'
			});

			const body = JSON.parse(mockFetch.mock.calls[0][1].body);
			expect(body.status).toBe('wanted');
			expect(body.notes).toBe('Looking for this');
			expect(result.status).toBe('wanted');
		});

		it('throws on not found', async () => {
			mockFetch.mockResolvedValue(jsonResponse({ error: 'Entry not found' }, 404));

			await expect(updateCollectionEntry(999, { status: 'owned' })).rejects.toThrow(
				'Entry not found'
			);
		});
	});

	describe('deleteCollectionEntry', () => {
		it('deletes an entry successfully', async () => {
			mockFetch.mockResolvedValue({ ok: true, status: 204, json: () => Promise.resolve({}) });

			await expect(deleteCollectionEntry(1)).resolves.toBeUndefined();

			expect(mockFetch).toHaveBeenCalledWith('/api/v1/me/collection/1', {
				method: 'DELETE',
				credentials: 'same-origin'
			});
		});

		it('throws on not found', async () => {
			mockFetch.mockResolvedValue(jsonResponse({ error: 'Entry not found' }, 404));

			await expect(deleteCollectionEntry(999)).rejects.toThrow('Entry not found');
		});

		it('throws generic message on non-JSON error', async () => {
			mockFetch.mockResolvedValue({
				ok: false,
				status: 500,
				json: () => Promise.reject(new Error('not json'))
			});

			await expect(deleteCollectionEntry(1)).rejects.toThrow('An unexpected error occurred');
		});
	});

	describe('fetchCollectionStats', () => {
		it('fetches collection statistics', async () => {
			const stats = {
				total_issues: 620,
				total_owned: 150,
				total_duplicate: 5,
				total_wanted: 30,
				overall_progress_percent: 24.19,
				series_stats: [
					{
						series_id: 1,
						series_name: 'Maddrax',
						series_slug: 'maddrax',
						total_in_series: 620,
						owned_count: 150,
						duplicate_count: 5,
						wanted_count: 30,
						progress_percent: 24.19
					}
				]
			};
			mockFetch.mockResolvedValue(jsonResponse(stats));

			const result = await fetchCollectionStats();

			expect(mockFetch).toHaveBeenCalledWith('/api/v1/me/collection/stats', {
				credentials: 'same-origin'
			});
			expect(result.total_owned).toBe(150);
			expect(result.series_stats).toHaveLength(1);
		});

		it('throws on server error', async () => {
			mockFetch.mockResolvedValue(jsonResponse({ error: 'Internal error' }, 500));

			await expect(fetchCollectionStats()).rejects.toThrow('Internal error');
		});
	});

	describe('error handling', () => {
		it('handles non-JSON error response', async () => {
			mockFetch.mockResolvedValue({
				ok: false,
				status: 500,
				json: () => Promise.reject(new Error('not json'))
			});

			await expect(fetchCollection()).rejects.toThrow('An unexpected error occurred');
		});

		it('handles error response without error field', async () => {
			mockFetch.mockResolvedValue(jsonResponse({ message: 'something' }, 400));

			await expect(fetchCollection()).rejects.toThrow('An unexpected error occurred');
		});
	});
});
