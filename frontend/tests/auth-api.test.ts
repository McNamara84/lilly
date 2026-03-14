import { describe, it, expect, vi, beforeEach } from 'vitest';
import { login, register, fetchMe, refreshToken, logout, resendVerification } from '../src/lib/api/auth';

// Mock global fetch
const mockFetch = vi.fn();
vi.stubGlobal('fetch', mockFetch);

describe('Auth API Client', () => {
	beforeEach(() => {
		mockFetch.mockReset();
	});

	describe('login', () => {
		it('sends login request with credentials and correct payload', async () => {
			mockFetch.mockResolvedValue({
				ok: true,
				json: () => Promise.resolve({ message: 'Login successful' })
			});

			const result = await login({ email: 'test@test.com', password: 'password' });

			expect(mockFetch).toHaveBeenCalledWith('/api/v1/auth/login', {
				method: 'POST',
				headers: { 'Content-Type': 'application/json' },
				credentials: 'same-origin',
				body: JSON.stringify({ email: 'test@test.com', password: 'password' })
			});

			expect(result.message).toBe('Login successful');
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

		it('handles non-JSON error response', async () => {
			mockFetch.mockResolvedValue({
				ok: false,
				json: () => Promise.reject(new Error('Not JSON'))
			});

			await expect(login({ email: 'test@test.com', password: 'pwd' })).rejects.toThrow(
				'An unexpected error occurred'
			);
		});

		it('preserves error code from API response', async () => {
			mockFetch.mockResolvedValue({
				ok: false,
				json: () =>
					Promise.resolve({ error: 'Email not verified', code: 'EMAIL_NOT_VERIFIED' })
			});

			try {
				await login({ email: 'test@test.com', password: 'pwd' });
			} catch (err) {
				expect((err as Error & { code?: string }).code).toBe('EMAIL_NOT_VERIFIED');
			}
		});
	});

	describe('register', () => {
		it('sends register request with correct payload', async () => {
			mockFetch.mockResolvedValue({
				ok: true,
				json: () => Promise.resolve({ message: 'Registration successful.' })
			});

			const result = await register({
				display_name: 'Max',
				email: 'max@test.com',
				password: 'strongpass123!',
				password_confirmation: 'strongpass123!',
				privacy_consent: true
			});

			expect(mockFetch).toHaveBeenCalledWith('/api/v1/auth/register', {
				method: 'POST',
				headers: { 'Content-Type': 'application/json' },
				credentials: 'same-origin',
				body: expect.any(String)
			});
			expect(result.message).toContain('Registration successful');
		});
	});

	describe('fetchMe', () => {
		it('sends GET request with credentials', async () => {
			mockFetch.mockResolvedValue({
				ok: true,
				json: () =>
					Promise.resolve({
						id: 1,
						email: 'user@test.com',
						display_name: 'User',
						email_verified: true
					})
			});

			const result = await fetchMe();

			expect(mockFetch).toHaveBeenCalledWith('/api/v1/auth/me', {
				credentials: 'same-origin'
			});
			expect(result.display_name).toBe('User');
		});
	});

	describe('refreshToken', () => {
		it('sends POST request with credentials', async () => {
			mockFetch.mockResolvedValue({ ok: true });

			await refreshToken();

			expect(mockFetch).toHaveBeenCalledWith('/api/v1/auth/refresh', {
				method: 'POST',
				credentials: 'same-origin'
			});
		});

		it('throws on failure', async () => {
			mockFetch.mockResolvedValue({ ok: false });

			await expect(refreshToken()).rejects.toThrow('Token refresh failed');
		});
	});

	describe('logout', () => {
		it('sends POST request with credentials', async () => {
			mockFetch.mockResolvedValue({ ok: true });

			await logout();

			expect(mockFetch).toHaveBeenCalledWith('/api/v1/auth/logout', {
				method: 'POST',
				credentials: 'same-origin'
			});
		});
	});

	describe('resendVerification', () => {
		it('sends email in request body', async () => {
			mockFetch.mockResolvedValue({ ok: true });

			await resendVerification('user@test.com');

			expect(mockFetch).toHaveBeenCalledWith('/api/v1/auth/resend-verification', {
				method: 'POST',
				headers: { 'Content-Type': 'application/json' },
				credentials: 'same-origin',
				body: JSON.stringify({ email: 'user@test.com' })
			});
		});
	});

	describe('network errors', () => {
		it('propagates network errors', async () => {
			mockFetch.mockRejectedValue(new TypeError('Network error'));

			await expect(login({ email: 'test@test.com', password: 'pwd' })).rejects.toThrow(
				'Network error'
			);
		});
	});
});
