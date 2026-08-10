import { beforeEach, describe, expect, it, vi } from 'vitest';
import {
	fetchMessages,
	fetchMessageThreads,
	markThreadRead,
	sendMessage
} from '../src/lib/api/messages';

const mockFetch = vi.fn();
globalThis.fetch = mockFetch;

function response(data: unknown, status = 200) {
	return {
		ok: status >= 200 && status < 300,
		json: vi.fn().mockResolvedValue(data)
	};
}

describe('Messages API', () => {
	beforeEach(() => vi.clearAllMocks());

	it('lists threads and paginates messages', async () => {
		const threads = { data: [], page: 2, per_page: 10, total: 0 };
		const controller = new AbortController();
		mockFetch.mockResolvedValue(response(threads));

		await expect(
			fetchMessageThreads({ page: 2, per_page: 10 }, controller.signal)
		).resolves.toEqual(threads);
		expect(mockFetch).toHaveBeenLastCalledWith('/api/v1/me/messages?page=2&per_page=10', {
			credentials: 'same-origin',
			signal: controller.signal
		});

		const messages = { data: [], next_before_id: null };
		mockFetch.mockResolvedValue(response(messages));
		await expect(fetchMessages(7, { before_id: 30, limit: 20 })).resolves.toEqual(messages);
		expect(mockFetch).toHaveBeenLastCalledWith('/api/v1/me/messages/7?before_id=30&limit=20', {
			credentials: 'same-origin',
			signal: undefined
		});
	});

	it('sends an idempotent message and marks a thread read', async () => {
		const message = { id: 4, content: 'Hallo' };
		mockFetch.mockResolvedValue(response(message));

		const clientMessageId = '123e4567-e89b-12d3-a456-426614174000';
		await expect(sendMessage(7, 'Hallo', clientMessageId)).resolves.toEqual(message);
		expect(mockFetch).toHaveBeenLastCalledWith('/api/v1/me/messages/7', {
			method: 'POST',
			headers: { 'Content-Type': 'application/json' },
			credentials: 'same-origin',
			body: JSON.stringify({ client_message_id: clientMessageId, content: 'Hallo' })
		});

		mockFetch.mockResolvedValue({ ok: true });
		await expect(markThreadRead(7, 4)).resolves.toBeUndefined();
		expect(mockFetch).toHaveBeenLastCalledWith('/api/v1/me/messages/7/read', {
			method: 'PATCH',
			headers: { 'Content-Type': 'application/json' },
			credentials: 'same-origin',
			body: JSON.stringify({ through_message_id: 4 })
		});
	});

	it('uses server and fallback errors', async () => {
		mockFetch.mockResolvedValueOnce(response({ error: 'Kein Zugriff' }, 403));
		await expect(fetchMessages(9)).rejects.toThrow('Kein Zugriff');

		mockFetch.mockResolvedValueOnce({ ok: false, json: vi.fn().mockRejectedValue('invalid') });
		await expect(markThreadRead(9, 1)).rejects.toThrow('Ein Fehler ist aufgetreten.');
	});
});
