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
	source_url: string | null;
}

export interface Issue {
	id: number;
	series_id: number;
	issue_number: number;
	title: string;
	authors: string[];
	published_at: string | null;
	cycle: string | null;
	cover_artists: string[];
	keywords: string[];
	notes: string[];
	cover_url: string | null;
	cover_local_path: string | null;
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
	const response = await fetch(`${API_BASE}/series`, {
		credentials: 'same-origin'
	});
	return handleResponse<Series[]>(response);
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
}

export async function fetchIssue(id: number): Promise<Issue> {
	const response = await fetch(`${API_BASE}/issues/${id}`, {
		credentials: 'same-origin'
	});
	return handleResponse<Issue>(response);
}
