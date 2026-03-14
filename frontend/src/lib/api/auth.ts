export interface LoginRequest {
	email: string;
	password: string;
}

export interface LoginResponse {
	message: string;
}

export interface RegisterRequest {
	display_name: string;
	email: string;
	password: string;
	password_confirmation: string;
	privacy_consent: boolean;
}

export interface RegisterResponse {
	message: string;
}

export interface MeResponse {
	id: number;
	email: string;
	display_name: string;
	email_verified: boolean;
}

export interface ApiError {
	error: string;
	code?: string;
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
	await fetch(`${API_BASE}/auth/resend-verification`, {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		credentials: 'same-origin',
		body: JSON.stringify({ email })
	});
}
