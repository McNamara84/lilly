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
	edition_label: string | null;
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
	edition_label: string | null;
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
	scope?: 'open' | 'closed';
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

export interface TradePartner {
	id: number | null;
	display_name: string;
	avatar_path: string | null;
	location: string | null;
}

export interface MatchIssue {
	entry_id: number;
	wanted_entry_id: number;
	issue_id: number;
	issue_number: number;
	title: string;
	series_id: number;
	series_name: string;
	series_slug: string;
	cover_url: string | null;
	cover_local_path: string | null;
	copy_number: number;
	edition_label: string | null;
	wanted_edition_label: string | null;
	condition_grade: ConditionGrade;
}

export interface TradeMatch {
	id: number;
	status: 'active' | 'stale';
	revision: number;
	changed_at: string;
	partner: TradePartner;
	my_offers: MatchIssue[];
	partner_offers: MatchIssue[];
	match_score: number;
	open_trade_id: number | null;
	open_trade_status: TradeStatus | null;
}

export type TradeStatus = 'proposed' | 'accepted' | 'cancelled' | 'completed';

export interface TradeItem {
	entry_id: number | null;
	wanted_entry_id: number | null;
	issue_id: number;
	issue_number: number;
	title: string;
	series_id: number;
	series_name: string;
	series_slug: string;
	cover_url: string | null;
	cover_local_path: string | null;
	copy_number: number;
	edition_label: string | null;
	wanted_edition_label: string | null;
	condition_grade: ConditionGrade;
}

export interface Trade {
	id: number;
	match_id: number | null;
	status: TradeStatus;
	role: 'initiator' | 'responder';
	partner: TradePartner;
	my_offers: TradeItem[];
	partner_offers: TradeItem[];
	thread_id: number;
	cancellation_reason: string | null;
	proposed_at: string;
	accepted_at: string | null;
	cancelled_at: string | null;
	completed_at: string | null;
	my_completion_confirmed_at: string | null;
	partner_completion_confirmed_at: string | null;
	updated_at: string;
}

export interface PageParams {
	page?: number;
	per_page?: number;
}

export interface TradePageParams extends PageParams {
	scope?: 'open' | 'closed';
}

async function handleResponse<T>(response: Response): Promise<T> {
	if (!response.ok) {
		const body = await response.json().catch(() => ({ error: 'An unexpected error occurred' }));
		const error = new Error(
			typeof body?.error === 'string' && body.error ? body.error : 'An unexpected error occurred'
		);
		(error as Error & { status?: number; code?: string }).status = response.status;
		if (typeof body?.code === 'string')
			(error as Error & { status?: number; code?: string }).code = body.code;
		throw error;
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

export async function fetchMatches(
	params: PageParams = {},
	signal?: AbortSignal
): Promise<PaginatedResponse<TradeMatch>> {
	const response = await fetch(
		`${API_BASE}/me/matches${buildQueryString(params)}`,
		requestInit(signal)
	);
	return handleResponse<PaginatedResponse<TradeMatch>>(response);
}

export async function fetchMatch(matchId: number, signal?: AbortSignal): Promise<TradeMatch> {
	const response = await fetch(`${API_BASE}/me/matches/${matchId}`, requestInit(signal));
	return handleResponse<TradeMatch>(response);
}

export async function createTradeProposal(
	matchId: number,
	offeredEntryIds: number[],
	requestedEntryIds: number[]
): Promise<Trade> {
	const response = await fetch(`${API_BASE}/me/matches/${matchId}/proposals`, {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		credentials: 'same-origin',
		body: JSON.stringify({
			offered_entry_ids: offeredEntryIds,
			requested_entry_ids: requestedEntryIds
		})
	});
	return handleResponse<Trade>(response);
}

export async function fetchOpenTrades(
	params: PageParams = {},
	signal?: AbortSignal
): Promise<PaginatedResponse<Trade>> {
	const response = await fetch(
		`${API_BASE}/me/trades${buildQueryString(params)}`,
		requestInit(signal)
	);
	return handleResponse<PaginatedResponse<Trade>>(response);
}

export async function fetchClosedTrades(
	params: PageParams = {},
	signal?: AbortSignal
): Promise<PaginatedResponse<Trade>> {
	const response = await fetch(
		`${API_BASE}/me/trades${buildQueryString({ ...params, scope: 'closed' })}`,
		requestInit(signal)
	);
	return handleResponse<PaginatedResponse<Trade>>(response);
}

export async function fetchTrade(tradeId: number, signal?: AbortSignal): Promise<Trade> {
	const response = await fetch(`${API_BASE}/me/trades/${tradeId}`, requestInit(signal));
	return handleResponse<Trade>(response);
}

export async function acceptTrade(tradeId: number): Promise<Trade> {
	const response = await fetch(`${API_BASE}/me/trades/${tradeId}/accept`, {
		method: 'POST',
		credentials: 'same-origin'
	});
	return handleResponse<Trade>(response);
}

export async function cancelTrade(tradeId: number): Promise<void> {
	const response = await fetch(`${API_BASE}/me/trades/${tradeId}/cancel`, {
		method: 'POST',
		credentials: 'same-origin'
	});
	if (!response.ok) await handleResponse<never>(response);
}

export async function completeTrade(tradeId: number): Promise<Trade> {
	const response = await fetch(`${API_BASE}/me/trades/${tradeId}/complete`, {
		method: 'POST',
		credentials: 'same-origin'
	});
	return handleResponse<Trade>(response);
}
