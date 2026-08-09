import { beforeEach, describe, expect, it, vi } from 'vitest';
import {
	addWantedBulk,
	deleteWantedEntry,
	fetchTradeOffers,
	fetchWantedCandidates,
	fetchWantedEntries
} from '../src/lib/api/trades';

const mockFetch = vi.fn();
globalThis.fetch = mockFetch;

function jsonResponse(data: unknown, status = 200) {
	return {
		ok: status >= 200 && status < 300,
		status,
		json: () => Promise.resolve(data)
	};
}

describe('Trades API', () => {
	beforeEach(() => vi.clearAllMocks());

	it('fetches paginated trade offers without empty query parameters', async () => {
		const data = { data: [], page: 1, per_page: 50, total: 0 };
		mockFetch.mockResolvedValue(jsonResponse(data));

		await expect(fetchTradeOffers()).resolves.toEqual(data);
		expect(mockFetch).toHaveBeenCalledWith('/api/v1/me/trade-offers', {
			credentials: 'same-origin'
		});
	});

	it('serializes filters and passes an abort signal for offers', async () => {
		mockFetch.mockResolvedValue(jsonResponse({ data: [], page: 2, per_page: 25, total: 30 }));
		const controller = new AbortController();

		await fetchTradeOffers(
			{ series_slug: 'maddrax', q: 'ice', page: 2, per_page: 25 },
			controller.signal
		);

		expect(mockFetch).toHaveBeenCalledWith(
			'/api/v1/me/trade-offers?series_slug=maddrax&q=ice&page=2&per_page=25',
			expect.objectContaining({ credentials: 'same-origin', signal: controller.signal })
		);
	});

	it('fetches wanted entries and omits empty filters', async () => {
		const data = { data: [], page: 1, per_page: 50, total: 0 };
		mockFetch.mockResolvedValue(jsonResponse(data));

		await fetchWantedEntries({ q: '', series_slug: undefined });

		expect(mockFetch).toHaveBeenCalledWith('/api/v1/me/wanted', {
			credentials: 'same-origin'
		});
	});

	it('fetches wanted candidates for a selected series', async () => {
		const data = { data: [], page: 1, per_page: 50, total: 0 };
		mockFetch.mockResolvedValue(jsonResponse(data));

		await fetchWantedCandidates({ series_slug: 'john-sinclair', page: 1 });

		expect(mockFetch).toHaveBeenCalledWith(
			'/api/v1/me/wanted/candidates?series_slug=john-sinclair&page=1',
			{ credentials: 'same-origin' }
		);
	});

	it('posts a deduplicated selection payload unchanged to the bulk endpoint', async () => {
		const result = {
			created: [{ issue_id: 1, entry_id: 10 }],
			unchanged: [{ issue_id: 2, entry_id: 20 }],
			rejected: []
		};
		mockFetch.mockResolvedValue(jsonResponse(result));

		await expect(addWantedBulk([1, 2])).resolves.toEqual(result);
		expect(mockFetch).toHaveBeenCalledWith('/api/v1/me/wanted/bulk', {
			method: 'POST',
			headers: { 'Content-Type': 'application/json' },
			credentials: 'same-origin',
			body: JSON.stringify({ issue_ids: [1, 2] })
		});
	});

	it('deletes a wanted entry', async () => {
		mockFetch.mockResolvedValue({ ok: true, status: 204, json: vi.fn() });

		await expect(deleteWantedEntry(12)).resolves.toBeUndefined();
		expect(mockFetch).toHaveBeenCalledWith('/api/v1/me/wanted/12', {
			method: 'DELETE',
			credentials: 'same-origin'
		});
	});

	it('surfaces JSON and non-JSON API errors', async () => {
		mockFetch.mockResolvedValueOnce(jsonResponse({ error: 'Unauthorized' }, 401));
		await expect(fetchTradeOffers()).rejects.toThrow('Unauthorized');

		mockFetch.mockResolvedValueOnce({
			ok: false,
			status: 500,
			json: () => Promise.reject(new Error('not json'))
		});
		await expect(deleteWantedEntry(12)).rejects.toThrow('An unexpected error occurred');
	});

	it('uses a generic error when the response has no error message', async () => {
		mockFetch.mockResolvedValue(jsonResponse({ message: 'broken' }, 400));
		await expect(fetchWantedEntries()).rejects.toThrow('An unexpected error occurred');
	});
});
