export interface LoginRequest {
	email: string;
	password: string;
}

export interface LoginResponse {
	access_token: string;
	token_type: string;
	expires_in: number;
}

export interface ApiError {
	error: string;
}

const API_BASE = '/api/v1';

export async function login(credentials: LoginRequest): Promise<LoginResponse> {
	const response = await fetch(`${API_BASE}/auth/login`, {
		method: 'POST',
		headers: {
			'Content-Type': 'application/json'
		},
		body: JSON.stringify(credentials)
	});

	if (!response.ok) {
		const errorBody = await response.json().catch(() => ({}));
		const message =
			typeof errorBody?.error === 'string' && errorBody.error
				? errorBody.error
				: 'An unexpected error occurred';
		throw new Error(message);
	}

	return response.json();
}
