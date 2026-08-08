import type { ConditionGrade } from '$lib/collection/conditions';

const API_BASE = '/api/v1';

export interface TradeOffer {
	entry_id: number;
	issue_id: number;
	issue_number: number;
	title: string;
	series_id: number;
	series_name: string;
	series_slug: string;
	cover_url: string | null;
	cover_local_path: string | null;
	copy_number: number;
	condition_grade: ConditionGrade;
	offering_user_id: number;
	offering_user_display_name: string;
}

export interface WantedEntry {
	entry_id: number;
	issue_id: number;
	issue_number: number;
	title: string;
	series_id: number;
	series_name: string;
	series_slug: string;
	cover_url: string | null;
	cover_local_path: string | null;
	copy_number: number;
	condition_grade: ConditionGrade | null;
}

export interface WantedCandidate {
	issue_id: number;
	issue_number: number;
	title: string;
	series_id: number;
	series_name: string;
	series_slug: string;
	cover_url: string | null;
	cover_local_path: string | null;
	is_wanted: boolean;
	wanted_entry_id: number | null;
}

export interface PaginatedResponse<T> {
	data: T[];
	page: number;
	per_page: number;
	total: number;
}

export interface TradeListParams {
	series_slug?: string;
	q?: string;
	page?: number;
	per_page?: number;
}

export interface WantedMutationResult {
	issue_id: number;
	entry_id: number;
}

export interface WantedRejection {
	issue_id: number;
	reason: 'already_owned' | 'issue_not_found';
}

export interface BulkWantedResult {
	created: WantedMutationResult[];
	unchanged: WantedMutationResult[];
	rejected: WantedRejection[];
}

async function handleResponse<T>(response: Response): Promise<T> {
	if (!response.ok) {
		const body = await response.json().catch(() => ({ error: 'An unexpected error occurred' }));
		throw new Error(
			typeof body?.error === 'string' && body.error ? body.error : 'An unexpected error occurred'
		);
	}
	return response.json();
}

function buildQueryString(params: TradeListParams): string {
	const query = new URLSearchParams();
	for (const [key, value] of Object.entries(params)) {
		if (value !== undefined && value !== null && value !== '') query.set(key, String(value));
	}
	const serialized = query.toString();
	return serialized ? `?${serialized}` : '';
}

function requestInit(signal?: AbortSignal): RequestInit {
	const init: RequestInit = { credentials: 'same-origin' };
	if (signal) init.signal = signal;
	return init;
}

export async function fetchTradeOffers(
	params: TradeListParams = {},
	signal?: AbortSignal
): Promise<PaginatedResponse<TradeOffer>> {
	const response = await fetch(
		`${API_BASE}/me/trade-offers${buildQueryString(params)}`,
		requestInit(signal)
	);
	return handleResponse<PaginatedResponse<TradeOffer>>(response);
}

export async function fetchWantedEntries(
	params: TradeListParams = {},
	signal?: AbortSignal
): Promise<PaginatedResponse<WantedEntry>> {
	const response = await fetch(
		`${API_BASE}/me/wanted${buildQueryString(params)}`,
		requestInit(signal)
	);
	return handleResponse<PaginatedResponse<WantedEntry>>(response);
}

export async function fetchWantedCandidates(
	params: TradeListParams & { series_slug: string },
	signal?: AbortSignal
): Promise<PaginatedResponse<WantedCandidate>> {
	const response = await fetch(
		`${API_BASE}/me/wanted/candidates${buildQueryString(params)}`,
		requestInit(signal)
	);
	return handleResponse<PaginatedResponse<WantedCandidate>>(response);
}

export async function addWantedBulk(issueIds: number[]): Promise<BulkWantedResult> {
	const response = await fetch(`${API_BASE}/me/wanted/bulk`, {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		credentials: 'same-origin',
		body: JSON.stringify({ issue_ids: issueIds })
	});
	return handleResponse<BulkWantedResult>(response);
}

export async function deleteWantedEntry(entryId: number): Promise<void> {
	const response = await fetch(`${API_BASE}/me/wanted/${entryId}`, {
		method: 'DELETE',
		credentials: 'same-origin'
	});
	if (!response.ok) await handleResponse<never>(response);
}
