import { beforeEach, describe, expect, it, vi } from 'vitest';
import {
	acceptTrade,
	addWantedBulk,
	cancelTrade,
	completeTrade,
	createTradeProposal,
	deleteWantedEntry,
	fetchMatch,
	fetchMatches,
	fetchClosedTrades,
	fetchOpenTrades,
	fetchTrade,
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

	it('fetches match and trade resources with pagination and abort signals', async () => {
		const page = { data: [], page: 2, per_page: 10, total: 0 };
		const controller = new AbortController();
		mockFetch.mockResolvedValue(jsonResponse(page));

		await expect(fetchMatches({ page: 2, per_page: 10 }, controller.signal)).resolves.toEqual(page);
		expect(mockFetch).toHaveBeenLastCalledWith('/api/v1/me/matches?page=2&per_page=10', {
			credentials: 'same-origin',
			signal: controller.signal
		});

		await fetchOpenTrades({ per_page: 25 });
		expect(mockFetch).toHaveBeenLastCalledWith('/api/v1/me/trades?per_page=25', {
			credentials: 'same-origin'
		});

		await fetchClosedTrades({ page: 3, per_page: 25 }, controller.signal);
		expect(mockFetch).toHaveBeenLastCalledWith(
			'/api/v1/me/trades?page=3&per_page=25&scope=closed',
			{ credentials: 'same-origin', signal: controller.signal }
		);
	});

	it('fetches match and trade details', async () => {
		const detail = { id: 4 };
		mockFetch.mockResolvedValue(jsonResponse(detail));

		await expect(fetchMatch(4)).resolves.toEqual(detail);
		expect(mockFetch).toHaveBeenLastCalledWith('/api/v1/me/matches/4', {
			credentials: 'same-origin'
		});
		await expect(fetchTrade(9)).resolves.toEqual(detail);
		expect(mockFetch).toHaveBeenLastCalledWith('/api/v1/me/trades/9', {
			credentials: 'same-origin'
		});
	});

	it('creates, accepts, completes and cancels trade proposals', async () => {
		const created = { id: 9, status: 'proposed' };
		mockFetch.mockResolvedValue(jsonResponse(created));

		await expect(createTradeProposal(3, [10, 11], [20])).resolves.toEqual(created);
		expect(mockFetch).toHaveBeenLastCalledWith('/api/v1/me/matches/3/proposals', {
			method: 'POST',
			headers: { 'Content-Type': 'application/json' },
			credentials: 'same-origin',
			body: JSON.stringify({ offered_entry_ids: [10, 11], requested_entry_ids: [20] })
		});

		await expect(acceptTrade(9)).resolves.toEqual(created);
		expect(mockFetch).toHaveBeenLastCalledWith('/api/v1/me/trades/9/accept', {
			method: 'POST',
			credentials: 'same-origin'
		});

		await expect(completeTrade(9)).resolves.toEqual(created);
		expect(mockFetch).toHaveBeenLastCalledWith('/api/v1/me/trades/9/complete', {
			method: 'POST',
			credentials: 'same-origin'
		});

		mockFetch.mockResolvedValue({ ok: true, status: 204, json: vi.fn() });
		await expect(cancelTrade(9)).resolves.toBeUndefined();
		expect(mockFetch).toHaveBeenLastCalledWith('/api/v1/me/trades/9/cancel', {
			method: 'POST',
			credentials: 'same-origin'
		});
	});

	it('surfaces proposal and cancellation failures', async () => {
		mockFetch.mockResolvedValueOnce(jsonResponse({ error: 'Ungültige Auswahl' }, 422));
		await expect(createTradeProposal(3, [], [])).rejects.toThrow('Ungültige Auswahl');

		mockFetch.mockResolvedValueOnce(jsonResponse({ error: 'Bereits angenommen' }, 409));
		await expect(cancelTrade(9)).rejects.toThrow('Bereits angenommen');

		mockFetch.mockResolvedValueOnce(
			jsonResponse({ error: 'Sammlung wurde verändert', code: 'trade_items_changed' }, 409)
		);
		const completionError = await completeTrade(9).catch((error) => error);
		expect(completionError).toMatchObject({
			message: 'Sammlung wurde verändert',
			status: 409,
			code: 'trade_items_changed'
		});
	});
});
