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
	source_key?: string | null;
	source_record_id?: string | null;
	source_url: string | null;
}

export interface Adapter {
	name: string;
	display_name: string;
	version: string;
	source_key?: string;
}

export type ImportJobStatus =
	| 'pending'
	| 'running'
	| 'completed'
	| 'completed_with_errors'
	| 'failed'
	| 'cancelled'
	| 'interrupted';

export interface ImportJob {
	id: number;
	series_id: number;
	series_slug: string;
	adapter_name: string;
	source_key?: string | null;
	trigger_type: 'manual' | 'scheduled';
	scheduled_for: string | null;
	status: string;
	total_issues: number;
	imported_issues: number;
	created_issues?: number;
	updated_issues?: number;
	unchanged_issues?: number;
	skipped_issues?: number;
	failed_issues: number;
	error_message: string | null;
	started_by: number | null;
	started_at: string | null;
	completed_at: string | null;
	created_at?: string;
	updated_at?: string;
	cancel_requested_at?: string | null;
	retry_of_job_id?: number | null;
}

export interface ImportJobError {
	id: number;
	job_id: number;
	source_key: string;
	issue_number: number | null;
	source_record_id: string | null;
	stage: string;
	message: string;
	created_at: string;
}

export interface PaginatedImportErrors {
	data: ImportJobError[];
	page: number;
	per_page: number;
	total: number;
}

export interface ImportScheduleStatus {
	enabled: boolean;
	schedule: string;
	timezone: string;
	adapters: string[];
	next_run: string | null;
}

export interface IssueAdmin {
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

export async function cancelImport(id: number): Promise<ImportJob> {
	const response = await fetch(`${API_BASE}/admin/import/${id}/cancel`, {
		method: 'POST',
		credentials: 'same-origin'
	});
	return handleResponse<ImportJob>(response);
}

export async function retryImport(id: number): Promise<ImportJob> {
	const response = await fetch(`${API_BASE}/admin/import/${id}/retry`, {
		method: 'POST',
		credentials: 'same-origin'
	});
	return handleResponse<ImportJob>(response);
}

export async function fetchImportErrors(
	id: number,
	page: number = 1
): Promise<PaginatedImportErrors> {
	const response = await fetch(`${API_BASE}/admin/import/${id}/errors?page=${page}`, {
		credentials: 'same-origin'
	});
	return handleResponse<PaginatedImportErrors>(response);
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

export async function fetchImportSchedule(): Promise<ImportScheduleStatus> {
	const response = await fetch(`${API_BASE}/admin/import/schedule`, {
		credentials: 'same-origin'
	});
	return handleResponse<ImportScheduleStatus>(response);
}
