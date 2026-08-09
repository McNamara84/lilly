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
	latest_import_job_id?: number | null;
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
	severity?: 'info' | 'warning' | 'blocking';
	code?: string;
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

export type ReviewOutcome =
	'not_processed' | 'created' | 'updated' | 'unchanged' | 'skipped' | 'failed';
export type ReviewSeverity = 'info' | 'warning' | 'blocking';
export type CoverStatus =
	| 'imported'
	| 'reused'
	| 'missing_at_source'
	| 'not_permitted'
	| 'fetch_failed'
	| 'invalid'
	| 'storage_failed'
	| 'not_checked';

export interface ReviewItem {
	id: number;
	job_id: number;
	issue_id: number | null;
	issue_number: number;
	outcome: ReviewOutcome;
	severity: ReviewSeverity;
	stage: string | null;
	message: string | null;
	source_key: string;
	source_record_id: string | null;
	source_url: string | null;
	title: string | null;
	authors: string[];
	cover_artists: string[];
	published_at: string | null;
	part_number: number | null;
	part_total: number | null;
	cycle: string | null;
	cover_status: CoverStatus;
	cover_reason: string | null;
	cover_local_path: string | null;
	processed_at: string | null;
}

export interface PaginatedReviewItems {
	items: ReviewItem[];
	total: number;
	page: number;
	per_page: number;
}

export interface ReviewOutcomeCounts {
	total: number;
	not_processed: number;
	created: number;
	updated: number;
	unchanged: number;
	skipped: number;
	failed: number;
}

export interface EligibilityReason {
	code: string;
	message: string;
}

export interface ReferenceCheck {
	issue_number: number;
	expected_title: string;
	expected_authors: string[];
	expected_published_at: string;
	status: 'passed' | 'failed';
	message: string | null;
}

export interface PublicationEvent {
	id: number;
	series_id: number;
	import_job_id: number | null;
	actor_user_id: number;
	action: 'activated' | 'deactivated';
	decision: 'clean' | 'warnings_acknowledged' | null;
	warning_count: number;
	blocking_count: number;
	created_at: string;
}

export interface ImportReviewSummary {
	job_id: number;
	series_id: number;
	series_name: string;
	series_slug: string;
	series_active: boolean;
	job_status: string;
	outcomes: ReviewOutcomeCounts;
	warning_count: number;
	blocking_count: number;
	eligibility: {
		eligible: boolean;
		requires_acknowledgement: boolean;
		reasons: EligibilityReason[];
	};
	reference_checks: ReferenceCheck[];
	sample_issue_numbers: number[];
	last_publication_event: PublicationEvent | null;
}

export interface ReviewItemFilters {
	page?: number;
	perPage?: number;
	query?: string;
	outcome?: ReviewOutcome | '';
	severity?: ReviewSeverity | '';
	coverStatus?: CoverStatus | '';
	sample?: boolean;
}

export interface ActivationResponse {
	series_id: number;
	active: boolean;
	event: PublicationEvent | null;
}

export class AdminApiError extends Error {
	constructor(
		message: string,
		public readonly status: number,
		public readonly code: string | null
	) {
		super(message);
		this.name = 'AdminApiError';
	}
}

async function handleResponse<T>(response: Response): Promise<T> {
	if (!response.ok) {
		const errorBody = await response
			.json()
			.catch(() => ({ error: 'An unexpected error occurred' }));
		throw new AdminApiError(
			typeof errorBody?.error === 'string' && errorBody.error
				? errorBody.error
				: 'An unexpected error occurred',
			response.status,
			typeof errorBody?.code === 'string' ? errorBody.code : null
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

export async function fetchImportReviewSummary(id: number): Promise<ImportReviewSummary> {
	const response = await fetch(`${API_BASE}/admin/import/${id}/review/summary`, {
		credentials: 'same-origin'
	});
	return handleResponse<ImportReviewSummary>(response);
}

export async function fetchImportReviewItems(
	id: number,
	filters: ReviewItemFilters = {}
): Promise<PaginatedReviewItems> {
	const parameters = new URLSearchParams({
		page: String(filters.page ?? 1),
		per_page: String(filters.perPage ?? 50)
	});
	if (filters.query?.trim()) parameters.set('q', filters.query.trim());
	if (filters.outcome) parameters.set('outcome', filters.outcome);
	if (filters.severity) parameters.set('severity', filters.severity);
	if (filters.coverStatus) parameters.set('cover_status', filters.coverStatus);
	if (filters.sample) parameters.set('sample', 'true');
	const response = await fetch(`${API_BASE}/admin/import/${id}/review/items?${parameters}`, {
		credentials: 'same-origin'
	});
	return handleResponse<PaginatedReviewItems>(response);
}

export async function activateImport(
	id: number,
	acknowledgeWarnings: boolean
): Promise<ActivationResponse> {
	const response = await fetch(`${API_BASE}/admin/import/${id}/activate`, {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		credentials: 'same-origin',
		body: JSON.stringify({ acknowledge_warnings: acknowledgeWarnings })
	});
	return handleResponse<ActivationResponse>(response);
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
