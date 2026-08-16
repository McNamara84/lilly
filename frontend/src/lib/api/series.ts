import {
	getCachedProfile,
	getStoredIssue,
	listStoredIssues,
	listStoredSeries
} from '$lib/offline/database';
import { isNetworkFailure } from '$lib/offline/network';

const API_BASE = '/api/v1';

export interface Series {
	id: number;
	name: string;
	slug: string;
	publisher: string | null;
	genre: string | null;
	frequency: string | null;
	total_issues: number | null;
	status: string;
	active: boolean;
	source_key?: string | null;
	source_record_id?: string | null;
	source_url: string | null;
}

export interface Issue {
	id: number;
	series_id: number;
	issue_number: number;
	title: string;
	authors: string[];
	published_at: string | null;
	part_number: number | null;
	part_total: number | null;
	cycle: string | null;
	cover_artists: string[];
	keywords: string[];
	notes: string[];
	cover_url: string | null;
	cover_local_path: string | null;
	source_key?: string | null;
	source_record_id?: string | null;
	source_wiki_url: string | null;
}

export interface PaginatedIssues {
	data: Issue[];
	page: number;
	per_page: number;
	total: number;
}

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

export async function fetchSeries(): Promise<Series[]> {
	try {
		const response = await fetch(`${API_BASE}/series`, {
			credentials: 'same-origin'
		});
		return handleResponse<Series[]>(response);
	} catch (error) {
		if (!isNetworkFailure(error)) throw error;
		const profile = await getCachedProfile().catch(() => null);
		if (!profile) throw error;
		return listStoredSeries(profile.id);
	}
}

export async function fetchSeriesIssues(
	slug: string,
	page: number = 1,
	per_page?: number
): Promise<PaginatedIssues> {
	const params = new URLSearchParams({ page: String(page) });
	if (per_page !== undefined) params.set('per_page', String(per_page));
	const response = await fetch(
		`${API_BASE}/series/${encodeURIComponent(slug)}/issues?${params.toString()}`,
		{
			credentials: 'same-origin'
		}
	);
	return handleResponse<PaginatedIssues>(response);
}

export async function fetchAllSeriesIssues(slug: string): Promise<Issue[]> {
	try {
		const allIssues: Issue[] = [];
		let page = 1;
		let total = Infinity;

		while (allIssues.length < total) {
			const result = await fetchSeriesIssues(slug, page, 100);
			allIssues.push(...result.data);
			total = result.total;
			if (result.data.length === 0) break;
			page++;
		}

		return allIssues;
	} catch (error) {
		if (!isNetworkFailure(error)) throw error;
		const profile = await getCachedProfile().catch(() => null);
		if (!profile) throw error;
		const series = (await listStoredSeries(profile.id)).find((item) => item.slug === slug);
		if (!series) return [];
		return listStoredIssues(profile.id, series.id);
	}
}

export async function fetchIssue(id: number): Promise<Issue> {
	try {
		const response = await fetch(`${API_BASE}/issues/${id}`, {
			credentials: 'same-origin'
		});
		return handleResponse<Issue>(response);
	} catch (error) {
		if (!isNetworkFailure(error)) throw error;
		const profile = await getCachedProfile().catch(() => null);
		if (!profile) throw error;
		const issue = await getStoredIssue(profile.id, id);
		if (!issue) throw error;
		return issue;
	}
}
