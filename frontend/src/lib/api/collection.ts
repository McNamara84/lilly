import type { ConditionGrade } from '$lib/collection/conditions';

const API_BASE = '/api/v1';

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface CollectionEntry {
	id: number;
	issue_id: number;
	issue_number: number;
	title: string;
	series_id: number;
	series_name: string;
	series_slug: string;
	cover_url: string | null;
	cover_local_path: string | null;
	copy_number: number | null;
	edition_label: string | null;
	condition_grade: ConditionGrade | null;
	status: CollectionStatus;
	notes: string | null;
	created_at: string | null;
	updated_at: string | null;
}

export interface PaginatedCollection {
	data: CollectionEntry[];
	page: number;
	per_page: number;
	total: number;
}

export interface CollectionStats {
	total_issues: number | null;
	total_physical_owned: number;
	total_owned: number;
	total_duplicate: number;
	total_wanted: number;
	overall_progress_percent: number | null;
	series_stats: SeriesStats[];
}

export interface SeriesStats {
	series_id: number;
	series_name: string;
	series_slug: string;
	total_in_series: number | null;
	owned_count: number;
	duplicate_count: number;
	wanted_count: number;
	progress_percent: number | null;
}

export interface AddCollectionEntryRequest {
	issue_id: number;
	condition_grade?: ConditionGrade;
	status?: PersistedCollectionStatus;
	notes?: string;
	copy_number?: number;
	edition_label?: string;
}

export interface UpdateCollectionEntryRequest {
	condition_grade?: ConditionGrade;
	status?: PersistedCollectionStatus;
	notes?: string;
	edition_label?: string;
}

export type PersistedCollectionStatus = 'owned' | 'duplicate' | 'wanted';
export type CollectionStatus = PersistedCollectionStatus | 'missing';

export interface CollectionQueryParams {
	series_slug?: string;
	issue_id?: number;
	status?: string;
	issue_number?: number;
	condition?: string;
	title?: string;
	author?: string;
	condition_min?: string;
	condition_max?: string;
	sort?: string;
	sort_dir?: string;
	q?: string;
	page?: number;
	per_page?: number;
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

async function handleResponse<T>(response: Response): Promise<T> {
	if (!response.ok) {
		const errorBody = await response
			.json()
			.catch(() => ({ error: 'An unexpected error occurred' }));
		throw new Error(
			typeof errorBody?.error === 'string' && errorBody.error
				? errorBody.error
				: 'An unexpected error occurred'
		);
	}
	return response.json();
}

function buildQueryString(params: CollectionQueryParams): string {
	const entries = Object.entries(params).filter(
		([, v]) => v !== undefined && v !== null && v !== ''
	);
	if (entries.length === 0) return '';
	const searchParams = new URLSearchParams();
	for (const [key, value] of entries) {
		searchParams.set(key, String(value));
	}
	return `?${searchParams.toString()}`;
}

// ---------------------------------------------------------------------------
// API functions
// ---------------------------------------------------------------------------

export async function fetchCollection(
	params: CollectionQueryParams = {},
	signal?: AbortSignal
): Promise<PaginatedCollection> {
	const qs = buildQueryString(params);
	const init: RequestInit = { credentials: 'same-origin' };
	if (signal) init.signal = signal;
	const response = await fetch(`${API_BASE}/me/collection${qs}`, init);
	return handleResponse<PaginatedCollection>(response);
}

export async function fetchAllCollectionEntries(seriesSlug: string): Promise<CollectionEntry[]> {
	const allEntries: CollectionEntry[] = [];
	let page = 1;
	let total = Infinity;

	while (allEntries.length < total) {
		const result = await fetchCollection({ series_slug: seriesSlug, per_page: 100, page });
		allEntries.push(...result.data);
		total = result.total;
		if (result.data.length === 0) break;
		page++;
	}

	return allEntries;
}

export async function addToCollection(body: AddCollectionEntryRequest): Promise<CollectionEntry> {
	const response = await fetch(`${API_BASE}/me/collection`, {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		credentials: 'same-origin',
		body: JSON.stringify(body)
	});
	return handleResponse<CollectionEntry>(response);
}

export async function updateCollectionEntry(
	id: number,
	body: UpdateCollectionEntryRequest
): Promise<CollectionEntry> {
	const response = await fetch(`${API_BASE}/me/collection/${id}`, {
		method: 'PATCH',
		headers: { 'Content-Type': 'application/json' },
		credentials: 'same-origin',
		body: JSON.stringify(body)
	});
	return handleResponse<CollectionEntry>(response);
}

export async function deleteCollectionEntry(id: number): Promise<void> {
	const response = await fetch(`${API_BASE}/me/collection/${id}`, {
		method: 'DELETE',
		credentials: 'same-origin'
	});
	if (!response.ok) {
		const errorBody = await response
			.json()
			.catch(() => ({ error: 'An unexpected error occurred' }));
		throw new Error(
			typeof errorBody?.error === 'string' && errorBody.error
				? errorBody.error
				: 'An unexpected error occurred'
		);
	}
}

export async function fetchCollectionStats(): Promise<CollectionStats> {
	const response = await fetch(`${API_BASE}/me/collection/stats`, {
		credentials: 'same-origin'
	});
	return handleResponse<CollectionStats>(response);
}

export async function fetchCollectionEntryByIssue(
	issueId: number
): Promise<CollectionEntry | null> {
	const response = await fetch(`${API_BASE}/me/collection/by-issue/${issueId}`, {
		credentials: 'same-origin'
	});
	return handleResponse<CollectionEntry | null>(response);
}

export async function fetchCollectionEntriesByIssue(issueId: number): Promise<CollectionEntry[]> {
	const result = await fetchCollection({ issue_id: issueId, page: 1, per_page: 100 });
	return result.data;
}
