export interface LoginRequest {
	email: string;
	password: string;
}

export interface LoginResponse {
	message: string;
	account_state?: 'pending_deletion';
	scheduled_for?: string;
}

export interface RegisterRequest {
	display_name: string;
	email: string;
	password: string;
	password_confirmation: string;
	privacy_consent: boolean;
	privacy_policy_version: string;
}

export interface RegisterResponse {
	message: string;
}

export interface MeResponse {
	id: number;
	email: string;
	display_name: string;
	email_verified: boolean;
	role: 'user' | 'admin';
}

export interface ApiError {
	error: string;
	code?: string;
	fields?: Record<string, string>;
	retry_after_seconds?: number;
}

export interface PasswordResetConfirmRequest {
	token: string;
	password: string;
	password_confirmation: string;
}

export type OAuthProvider = 'google' | 'github';
export type OAuthIntent = 'login' | 'register' | 'reauth';

export interface AuthOptionsResponse {
	privacy_policy: {
		version: string;
		url: string;
	};
	oauth: Record<OAuthProvider, boolean>;
}

export interface PendingOAuthLink {
	pending: boolean;
	provider?: OAuthProvider;
	masked_email?: string;
	expires_at?: string;
	confirmation_token?: string;
}

export interface PrivacyConsent {
	policy_version: string;
	consented_at: string;
	registration_method: 'password' | OAuthProvider | 'legacy';
}

const API_BASE = '/api/v1';

async function handleResponse<T>(response: Response): Promise<T> {
	if (!response.ok) {
		const errorBody: ApiError = await response
			.json()
			.catch(() => ({ error: 'An unexpected error occurred' }));
		const error = new Error(
			typeof errorBody?.error === 'string' && errorBody.error
				? errorBody.error
				: 'An unexpected error occurred'
		);
		(error as ApiError & Error).code = errorBody?.code;
		(error as ApiError & Error).fields = errorBody?.fields;
		(error as ApiError & Error).retry_after_seconds = errorBody?.retry_after_seconds;
		throw error;
	}
	return response.json();
}

export async function login(credentials: LoginRequest): Promise<LoginResponse> {
	const response = await fetch(`${API_BASE}/auth/login`, {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		credentials: 'same-origin',
		body: JSON.stringify(credentials)
	});
	return handleResponse<LoginResponse>(response);
}

export async function register(data: RegisterRequest): Promise<RegisterResponse> {
	const response = await fetch(`${API_BASE}/auth/register`, {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		credentials: 'same-origin',
		body: JSON.stringify(data)
	});
	return handleResponse<RegisterResponse>(response);
}

export async function fetchAuthOptions(): Promise<AuthOptionsResponse> {
	const response = await fetch(`${API_BASE}/auth/options`, { credentials: 'same-origin' });
	return handleResponse<AuthOptionsResponse>(response);
}

export async function startOAuth(
	provider: OAuthProvider,
	intent: OAuthIntent,
	consent?: { privacy_consent: boolean; privacy_policy_version: string }
): Promise<string> {
	const response = await fetch(`${API_BASE}/auth/oauth/${provider}/start`, {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		credentials: 'same-origin',
		body: JSON.stringify({ intent, ...consent })
	});
	const result = await handleResponse<{ authorization_url: string }>(response);
	return result.authorization_url;
}

export async function fetchPendingOAuthLink(): Promise<PendingOAuthLink> {
	const response = await fetch(`${API_BASE}/auth/oauth/link`, { credentials: 'same-origin' });
	return handleResponse<PendingOAuthLink>(response);
}

export async function confirmOAuthLink(confirmationToken: string): Promise<void> {
	const response = await fetch(`${API_BASE}/auth/oauth/link`, {
		method: 'POST',
		headers: {
			'Content-Type': 'application/json',
			'X-CSRF-Token': confirmationToken
		},
		credentials: 'same-origin',
		body: '{}'
	});
	await handleResponse<{ message: string }>(response);
}

export async function cancelOAuthLink(): Promise<void> {
	const response = await fetch(`${API_BASE}/auth/oauth/link`, {
		method: 'DELETE',
		credentials: 'same-origin'
	});
	if (!response.ok && response.status !== 204) await handleResponse(response);
}

export async function fetchPrivacyConsents(): Promise<PrivacyConsent[]> {
	const response = await fetch(`${API_BASE}/me/privacy-consents`, {
		credentials: 'same-origin'
	});
	return handleResponse<PrivacyConsent[]>(response);
}

export async function fetchMe(): Promise<MeResponse> {
	const response = await fetch(`${API_BASE}/auth/me`, {
		credentials: 'same-origin'
	});
	return handleResponse<MeResponse>(response);
}

export async function refreshToken(): Promise<void> {
	const response = await fetch(`${API_BASE}/auth/refresh`, {
		method: 'POST',
		credentials: 'same-origin'
	});
	if (!response.ok) {
		throw new Error('Token refresh failed');
	}
}

export async function logout(): Promise<void> {
	await fetch(`${API_BASE}/auth/logout`, {
		method: 'POST',
		credentials: 'same-origin'
	});
}

export async function resendVerification(email: string): Promise<void> {
	const response = await fetch(`${API_BASE}/auth/resend-verification`, {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		credentials: 'same-origin',
		body: JSON.stringify({ email })
	});
	await handleResponse<{ message: string }>(response);
}

export async function requestPasswordReset(email: string): Promise<{ message: string }> {
	const response = await fetch(`${API_BASE}/auth/password-reset/request`, {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		credentials: 'same-origin',
		body: JSON.stringify({ email })
	});
	return handleResponse<{ message: string }>(response);
}

export async function confirmPasswordReset(
	data: PasswordResetConfirmRequest
): Promise<{ message: string }> {
	const response = await fetch(`${API_BASE}/auth/password-reset/confirm`, {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		credentials: 'same-origin',
		body: JSON.stringify(data)
	});
	return handleResponse<{ message: string }>(response);
}
