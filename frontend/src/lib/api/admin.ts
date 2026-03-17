const API_BASE = '/api/v1';

export interface SeriesAdmin {
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

export interface Adapter {
	name: string;
	display_name: string;
	version: string;
}

export interface ImportJob {
	id: number;
	series_id: number;
	series_slug: string;
	adapter_name: string;
	status: string;
	total_issues: number;
	imported_issues: number;
	error_message: string | null;
	started_by: number;
	started_at: string | null;
	completed_at: string | null;
}

export interface IssueAdmin {
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
	data: IssueAdmin[];
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

export async function fetchAllSeries(): Promise<SeriesAdmin[]> {
	const response = await fetch(`${API_BASE}/admin/series`, {
		credentials: 'same-origin'
	});
	return handleResponse<SeriesAdmin[]>(response);
}

export async function activateSeries(slug: string): Promise<void> {
	const response = await fetch(`${API_BASE}/admin/series/${encodeURIComponent(slug)}/activate`, {
		method: 'POST',
		credentials: 'same-origin'
	});
	await handleResponse(response);
}

export async function deactivateSeries(slug: string): Promise<void> {
	const response = await fetch(`${API_BASE}/admin/series/${encodeURIComponent(slug)}/deactivate`, {
		method: 'POST',
		credentials: 'same-origin'
	});
	await handleResponse(response);
}

export async function fetchAdapters(): Promise<Adapter[]> {
	const response = await fetch(`${API_BASE}/admin/adapters`, {
		credentials: 'same-origin'
	});
	return handleResponse<Adapter[]>(response);
}

export async function startImport(adapterName: string): Promise<ImportJob> {
	const response = await fetch(`${API_BASE}/admin/import`, {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		credentials: 'same-origin',
		body: JSON.stringify({ adapter: adapterName })
	});
	return handleResponse<ImportJob>(response);
}

export async function fetchImportJob(id: number): Promise<ImportJob> {
	const response = await fetch(`${API_BASE}/admin/import/${id}`, {
		credentials: 'same-origin'
	});
	return handleResponse<ImportJob>(response);
}

export async function fetchImportSeriesIssues(
	id: number,
	page: number = 1
): Promise<PaginatedIssues> {
	const response = await fetch(`${API_BASE}/admin/import/${id}/series-issues?page=${page}`, {
		credentials: 'same-origin'
	});
	return handleResponse<PaginatedIssues>(response);
}

export async function fetchImportHistory(): Promise<ImportJob[]> {
	const response = await fetch(`${API_BASE}/admin/import/history`, {
		credentials: 'same-origin'
	});
	return handleResponse<ImportJob[]>(response);
}
