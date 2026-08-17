import type { PaginatedResponse, TradePartner } from '$lib/api/trades';

const API_BASE = '/api/v1';

export interface MessageThreadSummary {
	id: number;
	trade_id: number;
	trade_status: string;
	partner: TradePartner;
	last_message: string | null;
	last_message_at: string | null;
	unread_count: number;
	updated_at: string;
}

export interface TradeMessage {
	id: number;
	thread_id: number;
	sender_id: number | null;
	content: string;
	created_at: string;
	read_at: string | null;
	is_mine: boolean;
}

export interface MessagePage {
	data: TradeMessage[];
	next_before_id: number | null;
}

async function handleResponse<T>(response: Response): Promise<T> {
	if (!response.ok) {
		const body = await response.json().catch(() => ({ error: 'Ein Fehler ist aufgetreten.' }));
		throw new Error(typeof body?.error === 'string' ? body.error : 'Ein Fehler ist aufgetreten.');
	}
	return response.json();
}

export async function fetchMessageThreads(
	params: { page?: number; per_page?: number } = {},
	signal?: AbortSignal
): Promise<PaginatedResponse<MessageThreadSummary>> {
	const query = new URLSearchParams();
	for (const [key, value] of Object.entries(params)) {
		if (value !== undefined) query.set(key, String(value));
	}
	const response = await fetch(`${API_BASE}/me/messages${query.size ? `?${query}` : ''}`, {
		credentials: 'same-origin',
		signal
	});
	return handleResponse<PaginatedResponse<MessageThreadSummary>>(response);
}

export async function fetchMessages(
	threadId: number,
	params: { before_id?: number; limit?: number } = {},
	signal?: AbortSignal
): Promise<MessagePage> {
	const query = new URLSearchParams();
	for (const [key, value] of Object.entries(params)) {
		if (value !== undefined) query.set(key, String(value));
	}
	const response = await fetch(
		`${API_BASE}/me/messages/${threadId}${query.size ? `?${query}` : ''}`,
		{ credentials: 'same-origin', signal }
	);
	return handleResponse<MessagePage>(response);
}

export async function sendMessage(
	threadId: number,
	content: string,
	clientMessageId = crypto.randomUUID()
): Promise<TradeMessage> {
	const response = await fetch(`${API_BASE}/me/messages/${threadId}`, {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		credentials: 'same-origin',
		body: JSON.stringify({ client_message_id: clientMessageId, content })
	});
	return handleResponse<TradeMessage>(response);
}

export async function markThreadRead(threadId: number, throughMessageId: number): Promise<void> {
	const response = await fetch(`${API_BASE}/me/messages/${threadId}/read`, {
		method: 'PATCH',
		headers: { 'Content-Type': 'application/json' },
		credentials: 'same-origin',
		body: JSON.stringify({ through_message_id: throughMessageId })
	});
	if (!response.ok) await handleResponse<never>(response);
}
