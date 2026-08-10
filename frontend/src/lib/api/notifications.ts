import type { PaginatedResponse } from '$lib/api/trades';

const API_BASE = '/api/v1';

export type NotificationKind =
	| 'trade_match'
	| 'trade_match_updated'
	| 'trade_proposed'
	| 'trade_accepted'
	| 'trade_cancelled'
	| 'trade_message';

export interface AppNotification {
	id: number;
	kind: NotificationKind;
	actor_user_id: number | null;
	match_id: number | null;
	trade_id: number | null;
	message_id: number | null;
	payload: Record<string, unknown>;
	read_at: string | null;
	created_at: string;
}

async function handleResponse<T>(response: Response): Promise<T> {
	if (!response.ok) {
		const body = await response.json().catch(() => ({ error: 'Ein Fehler ist aufgetreten.' }));
		throw new Error(typeof body?.error === 'string' ? body.error : 'Ein Fehler ist aufgetreten.');
	}
	return response.json();
}

export async function fetchNotifications(
	params: { page?: number; per_page?: number; unread_only?: boolean } = {},
	signal?: AbortSignal
): Promise<PaginatedResponse<AppNotification>> {
	const query = new URLSearchParams();
	for (const [key, value] of Object.entries(params)) {
		if (value !== undefined) query.set(key, String(value));
	}
	const response = await fetch(`${API_BASE}/me/notifications${query.size ? `?${query}` : ''}`, {
		credentials: 'same-origin',
		signal
	});
	return handleResponse<PaginatedResponse<AppNotification>>(response);
}

export async function fetchUnreadNotificationCount(signal?: AbortSignal): Promise<number> {
	const response = await fetch(`${API_BASE}/me/notifications/unread-count`, {
		credentials: 'same-origin',
		signal
	});
	return (await handleResponse<{ unread_count: number }>(response)).unread_count;
}

export async function markNotificationRead(notificationId: number): Promise<void> {
	const response = await fetch(`${API_BASE}/me/notifications/${notificationId}/read`, {
		method: 'PATCH',
		credentials: 'same-origin'
	});
	if (!response.ok) await handleResponse<never>(response);
}

export async function markAllNotificationsRead(): Promise<void> {
	const response = await fetch(`${API_BASE}/me/notifications/read-all`, {
		method: 'POST',
		credentials: 'same-origin'
	});
	if (!response.ok) await handleResponse<never>(response);
}
