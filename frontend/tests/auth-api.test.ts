import { describe, it, expect, vi, beforeEach } from 'vitest';
import { login } from '../src/lib/api/auth';

// Mock global fetch
const mockFetch = vi.fn();
vi.stubGlobal('fetch', mockFetch);

describe('Auth API Client', () => {
	beforeEach(() => {
		mockFetch.mockReset();
	});

	it('sends login request with correct payload', async () => {
		mockFetch.mockResolvedValue({
			ok: true,
			json: () =>
				Promise.resolve({
					access_token: 'test-token',
					token_type: 'Bearer',
					expires_in: 900
				})
		});

		const result = await login({ email: 'test@test.com', password: 'password' });

		expect(mockFetch).toHaveBeenCalledWith('/api/v1/auth/login', {
			method: 'POST',
			headers: { 'Content-Type': 'application/json' },
			body: JSON.stringify({ email: 'test@test.com', password: 'password' })
		});

		expect(result.access_token).toBe('test-token');
		expect(result.token_type).toBe('Bearer');
	});

	it('throws error on 401 response', async () => {
		mockFetch.mockResolvedValue({
			ok: false,
			json: () => Promise.resolve({ error: 'Invalid email or password' })
		});

		await expect(login({ email: 'bad@test.com', password: 'wrong' })).rejects.toThrow(
			'Invalid email or password'
		);
	});

	it('handles network errors gracefully', async () => {
		mockFetch.mockRejectedValue(new TypeError('Network error'));

		await expect(login({ email: 'test@test.com', password: 'pwd' })).rejects.toThrow(
			'Network error'
		);
	});

	it('handles non-JSON error response', async () => {
		mockFetch.mockResolvedValue({
			ok: false,
			json: () => Promise.reject(new Error('Not JSON'))
		});

		await expect(login({ email: 'test@test.com', password: 'pwd' })).rejects.toThrow(
			'An unexpected error occurred'
		);
	});
});
