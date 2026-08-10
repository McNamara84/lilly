import { beforeEach, describe, expect, it, vi } from 'vitest';
import {
	fetchNotifications,
	fetchUnreadNotificationCount,
	markAllNotificationsRead,
	markNotificationRead
} from '../src/lib/api/notifications';

const mockFetch = vi.fn();
globalThis.fetch = mockFetch;

function response(data: unknown, status = 200) {
	return { ok: status >= 200 && status < 300, json: vi.fn().mockResolvedValue(data) };
}

describe('Notifications API', () => {
	beforeEach(() => vi.clearAllMocks());

	it('lists unread notifications and fetches the count', async () => {
		const page = { data: [], page: 1, per_page: 20, total: 0 };
		mockFetch.mockResolvedValue(response(page));

		await expect(fetchNotifications({ page: 1, unread_only: true })).resolves.toEqual(page);
		expect(mockFetch).toHaveBeenLastCalledWith('/api/v1/me/notifications?page=1&unread_only=true', {
			credentials: 'same-origin',
			signal: undefined
		});

		mockFetch.mockResolvedValue(response({ unread_count: 3 }));
		await expect(fetchUnreadNotificationCount()).resolves.toBe(3);
		expect(mockFetch).toHaveBeenLastCalledWith('/api/v1/me/notifications/unread-count', {
			credentials: 'same-origin',
			signal: undefined
		});
	});

	it('marks one or all notifications read', async () => {
		mockFetch.mockResolvedValue({ ok: true });

		await expect(markNotificationRead(4)).resolves.toBeUndefined();
		expect(mockFetch).toHaveBeenLastCalledWith('/api/v1/me/notifications/4/read', {
			method: 'PATCH',
			credentials: 'same-origin'
		});

		await expect(markAllNotificationsRead()).resolves.toBeUndefined();
		expect(mockFetch).toHaveBeenLastCalledWith('/api/v1/me/notifications/read-all', {
			method: 'POST',
			credentials: 'same-origin'
		});
	});

	it('surfaces API errors', async () => {
		mockFetch.mockResolvedValueOnce(response({ error: 'Nicht angemeldet' }, 401));
		await expect(fetchUnreadNotificationCount()).rejects.toThrow('Nicht angemeldet');

		mockFetch.mockResolvedValueOnce(response({ error: null }, 500));
		await expect(fetchNotifications()).rejects.toThrow('Ein Fehler ist aufgetreten.');
		expect(mockFetch).toHaveBeenLastCalledWith('/api/v1/me/notifications', {
			credentials: 'same-origin',
			signal: undefined
		});

		mockFetch.mockResolvedValueOnce(response({ error: 'Nicht gefunden' }, 404));
		await expect(markNotificationRead(99)).rejects.toThrow('Nicht gefunden');

		mockFetch.mockResolvedValueOnce({ ok: false, json: vi.fn().mockRejectedValue('bad') });
		await expect(markAllNotificationsRead()).rejects.toThrow('Ein Fehler ist aufgetreten.');
	});

	it('omits undefined list filters and forwards the count abort signal', async () => {
		const page = { data: [], page: 1, per_page: 20, total: 0 };
		mockFetch.mockResolvedValueOnce(response(page));
		await expect(
			fetchNotifications({ page: undefined, per_page: undefined, unread_only: undefined })
		).resolves.toEqual(page);
		expect(mockFetch).toHaveBeenLastCalledWith('/api/v1/me/notifications', {
			credentials: 'same-origin',
			signal: undefined
		});

		const controller = new AbortController();
		mockFetch.mockResolvedValueOnce(response({ unread_count: 0 }));
		await expect(fetchUnreadNotificationCount(controller.signal)).resolves.toBe(0);
		expect(mockFetch).toHaveBeenLastCalledWith('/api/v1/me/notifications/unread-count', {
			credentials: 'same-origin',
			signal: controller.signal
		});
	});
});
