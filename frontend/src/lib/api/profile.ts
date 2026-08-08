import type { CollectionStats, PersistedCollectionStatus } from '$lib/api/collection';
import type { ConditionGrade } from '$lib/collection/conditions';

const API_BASE = '/api/v1';

export interface OwnProfile {
	id: number;
	email: string;
	display_name: string;
	avatar_path: string | null;
	location: string | null;
	profile_public: boolean;
	collection_public: boolean;
	created_at: string;
}

export interface VisibilitySettings {
	profile_public: boolean;
	collection_public: boolean;
}

export interface PublicProfile {
	id: number;
	display_name: string;
	avatar_path: string | null;
	location: string | null;
	created_at: string;
}

export interface PublicCollectionEntry {
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
	status: PersistedCollectionStatus;
	notes: string | null;
}

export interface PaginatedPublicCollection {
	data: PublicCollectionEntry[];
	page: number;
	per_page: number;
	total: number;
}

async function handleResponse<T>(response: Response): Promise<T> {
	if (!response.ok) {
		const errorBody = await response
			.json()
			.catch(() => ({ error: 'An unexpected error occurred' }));
		const error = new Error(
			typeof errorBody?.error === 'string' && errorBody.error
				? errorBody.error
				: 'An unexpected error occurred'
		);
		(error as Error & { status?: number }).status = response.status;
		throw error;
	}
	return response.json();
}

export async function fetchOwnProfile(): Promise<OwnProfile> {
	const response = await fetch(`${API_BASE}/me/profile`, { credentials: 'same-origin' });
	return handleResponse<OwnProfile>(response);
}

export async function updateVisibility(settings: VisibilitySettings): Promise<VisibilitySettings> {
	const response = await fetch(`${API_BASE}/me/profile/visibility`, {
		method: 'PATCH',
		headers: { 'Content-Type': 'application/json' },
		credentials: 'same-origin',
		body: JSON.stringify(settings)
	});
	return handleResponse<VisibilitySettings>(response);
}

export async function fetchPublicProfile(userId: number): Promise<PublicProfile> {
	const response = await fetch(`${API_BASE}/users/${userId}/profile`);
	return handleResponse<PublicProfile>(response);
}

export async function fetchPublicCollection(
	userId: number,
	page = 1,
	perPage = 100
): Promise<PaginatedPublicCollection> {
	const params = new URLSearchParams({ page: String(page), per_page: String(perPage) });
	const response = await fetch(`${API_BASE}/users/${userId}/collection?${params}`);
	return handleResponse<PaginatedPublicCollection>(response);
}

export async function fetchPublicCollectionStats(userId: number): Promise<CollectionStats> {
	const response = await fetch(`${API_BASE}/users/${userId}/collection/stats`);
	return handleResponse<CollectionStats>(response);
}
