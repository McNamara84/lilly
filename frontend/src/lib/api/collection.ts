import type { ConditionGrade } from '$lib/collection/conditions';
import {
	getCachedProfile,
	listStoredCollectionEntries,
	mergeCollectionEntries,
	replaceOfflineSnapshot
} from '$lib/offline/database';
import {
	queueCollectionCreate,
	queueCollectionUpdate,
	readLocalCollection,
	synchronizeCollection
} from '$lib/offline/collection';
import { isNetworkFailure } from '$lib/offline/network';
import type { CollectionSyncResponse, OfflineSnapshot } from '$lib/offline/types';

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
	revision?: number | null;
	created_at: string | null;
	updated_at: string | null;
	sync_state?: 'pending' | 'synced' | 'conflict';
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
	base_revision?: number;
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

export class CollectionApiError extends Error {
	constructor(
		message: string,
		public readonly status: number,
		public readonly code?: string
	) {
		super(message);
		this.name = 'CollectionApiError';
	}
}

async function handleResponse<T>(response: Response): Promise<T> {
	if (!response.ok) {
		const errorBody = await response
			.json()
			.catch(() => ({ error: 'An unexpected error occurred' }));
		throw new CollectionApiError(
			typeof errorBody?.error === 'string' && errorBody.error
				? errorBody.error
				: 'An unexpected error occurred',
			response.status,
			typeof errorBody?.code === 'string' ? errorBody.code : undefined
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
	try {
		const response = await fetch(`${API_BASE}/me/collection${qs}`, init);
		const result = await handleResponse<PaginatedCollection>(response);
		const profile = await getCachedProfile().catch(() => null);
		if (profile) {
			void mergeCollectionEntries(
				profile.id,
				result.data.filter((entry) => entry.id > 0)
			).catch(() => undefined);
		}
		return result;
	} catch (error) {
		if (!isNetworkFailure(error)) throw error;
		const profile = await getCachedProfile().catch(() => null);
		if (!profile) throw error;
		return readLocalCollection(profile.id, params);
	}
}

async function fetchAllCollectionPages(
	params: Omit<CollectionQueryParams, 'page' | 'per_page'>
): Promise<CollectionEntry[]> {
	const allEntries: CollectionEntry[] = [];
	let page = 1;
	let total = Infinity;

	while (allEntries.length < total) {
		const result = await fetchCollection({ ...params, page, per_page: 100 });
		allEntries.push(...result.data);
		total = result.total;
		if (result.data.length === 0) break;
		page++;
	}

	return allEntries;
}

export async function fetchAllCollectionEntries(seriesSlug: string): Promise<CollectionEntry[]> {
	return fetchAllCollectionPages({ series_slug: seriesSlug });
}

async function addToCollectionOnline(body: AddCollectionEntryRequest): Promise<CollectionEntry> {
	const response = await fetch(`${API_BASE}/me/collection`, {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		credentials: 'same-origin',
		body: JSON.stringify(body)
	});
	return handleResponse<CollectionEntry>(response);
}

export async function addToCollection(body: AddCollectionEntryRequest): Promise<CollectionEntry> {
	const profile = await getCachedProfile().catch(() => null);
	if (!profile) return addToCollectionOnline(body);

	const queued = await queueCollectionCreate(profile.id, body);
	try {
		const response = await synchronizeCollection(profile.id, sendCollectionSync);
		const result = response?.results.find(
			(candidate) => candidate.mutation_id === queued.mutation.mutation_id
		);
		if (result?.status === 'rejected') throw new Error(result.error ?? 'Änderung abgelehnt');
		return result?.entry ?? queued.entry;
	} catch (error) {
		if (isNetworkFailure(error)) return queued.entry;
		throw error;
	}
}

async function updateCollectionEntryOnline(
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

export async function updateCollectionEntry(
	id: number,
	body: UpdateCollectionEntryRequest
): Promise<CollectionEntry> {
	const profile = await getCachedProfile().catch(() => null);
	if (!profile) return updateCollectionEntryOnline(id, body);

	const entries = await listStoredCollectionEntries(profile.id).catch(() => []);
	if (!entries.some((entry) => entry.id === id)) {
		const online = await updateCollectionEntryOnline(id, body);
		void mergeCollectionEntries(profile.id, [online]).catch(() => undefined);
		return online;
	}

	const queued = await queueCollectionUpdate(profile.id, id, body);
	try {
		const response = await synchronizeCollection(profile.id, sendCollectionSync);
		const result = response?.results.find(
			(candidate) => candidate.mutation_id === queued.mutation.mutation_id
		);
		if (result?.status === 'rejected') throw new Error(result.error ?? 'Änderung abgelehnt');
		return result?.entry ?? queued.entry;
	} catch (error) {
		if (isNetworkFailure(error)) return queued.entry;
		throw error;
	}
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
	return fetchAllCollectionPages({ issue_id: issueId });
}

export async function fetchOfflineSnapshot(): Promise<OfflineSnapshot> {
	const response = await fetch(`${API_BASE}/me/collection/offline-snapshot`, {
		credentials: 'same-origin'
	});
	return handleResponse<OfflineSnapshot>(response);
}

export async function refreshOfflineSnapshot(): Promise<void> {
	const snapshot = await fetchOfflineSnapshot();
	await replaceOfflineSnapshot(snapshot);
}

export async function sendCollectionSync(
	mutations: Record<string, unknown>[]
): Promise<CollectionSyncResponse> {
	const response = await fetch(`${API_BASE}/me/collection/sync`, {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		credentials: 'same-origin',
		body: JSON.stringify({ mutations })
	});
	return handleResponse<CollectionSyncResponse>(response);
}

export async function syncPendingCollectionChanges(): Promise<CollectionSyncResponse | null> {
	const profile = await getCachedProfile().catch(() => null);
	if (!profile) return null;
	return synchronizeCollection(profile.id, sendCollectionSync);
}
