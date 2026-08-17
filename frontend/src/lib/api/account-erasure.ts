import type { LoginResponse, OAuthProvider } from '$lib/api/auth';

const API_BASE = '/api/v1';

export interface AccountDeletionStatus {
	status: 'scheduled' | 'running' | 'storage_pending' | 'failed' | 'completed';
	requested_at: string;
	scheduled_for: string;
	can_cancel: boolean;
}

export interface AccountDeletionOptions {
	recent_authentication: boolean;
	password: boolean;
	google: boolean;
	github: boolean;
	confirmation_phrase: string;
	grace_days: number;
}

interface ApiErrorBody {
	error?: string;
	code?: string;
}

async function handleResponse<T>(response: Response): Promise<T> {
	if (!response.ok) {
		const body = (await response.json().catch(() => ({}))) as ApiErrorBody;
		const error = new Error(body.error || 'Ein unerwarteter Fehler ist aufgetreten.');
		(error as Error & { code?: string; status?: number }).code = body.code;
		(error as Error & { code?: string; status?: number }).status = response.status;
		throw error;
	}
	return response.json() as Promise<T>;
}

export async function fetchAccountDeletionOptions(): Promise<AccountDeletionOptions> {
	const response = await fetch(`${API_BASE}/me/account-deletion/options`, {
		credentials: 'same-origin'
	});
	return handleResponse<AccountDeletionOptions>(response);
}

export async function fetchAccountDeletionStatus(): Promise<AccountDeletionStatus> {
	const response = await fetch(`${API_BASE}/me/account-deletion`, {
		credentials: 'same-origin'
	});
	return handleResponse<AccountDeletionStatus>(response);
}

export async function reauthenticateWithPassword(password: string): Promise<void> {
	const response = await fetch(`${API_BASE}/auth/reauth/password`, {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		credentials: 'same-origin',
		body: JSON.stringify({ password })
	});
	await handleResponse<{ message: string }>(response);
}

export async function requestAccountDeletion(confirmation: string): Promise<AccountDeletionStatus> {
	const response = await fetch(`${API_BASE}/me/account-deletion`, {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		credentials: 'same-origin',
		body: JSON.stringify({ confirmation })
	});
	return handleResponse<AccountDeletionStatus>(response);
}

export async function cancelAccountDeletion(): Promise<LoginResponse> {
	const response = await fetch(`${API_BASE}/me/account-deletion`, {
		method: 'DELETE',
		credentials: 'same-origin'
	});
	return handleResponse<LoginResponse>(response);
}

export function availableOAuthMethods(options: AccountDeletionOptions): OAuthProvider[] {
	return (['google', 'github'] as const).filter((provider) => options[provider]);
}
