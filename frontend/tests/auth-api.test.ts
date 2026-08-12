import { describe, it, expect, vi, beforeEach } from 'vitest';
import {
	login,
	register,
	fetchAuthOptions,
	startOAuth,
	fetchPendingOAuthLink,
	confirmOAuthLink,
	cancelOAuthLink,
	fetchPrivacyConsents,
	fetchMe,
	refreshToken,
	logout,
	resendVerification
} from '../src/lib/api/auth';

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
				json: () => Promise.resolve({ error: 'Email not verified', code: 'EMAIL_NOT_VERIFIED' })
			});

			try {
				await login({ email: 'test@test.com', password: 'pwd' });
			} catch (err) {
				expect((err as Error & { code?: string }).code).toBe('EMAIL_NOT_VERIFIED');
			}
		});

		it('preserves field-specific validation errors', async () => {
			mockFetch.mockResolvedValue({
				ok: false,
				json: () =>
					Promise.resolve({
						error: 'Validation failed',
						fields: { email: 'Invalid email format' }
					})
			});

			try {
				await login({ email: 'invalid', password: 'pwd' });
			} catch (error) {
				expect((error as Error & { fields?: Record<string, string> }).fields).toEqual({
					email: 'Invalid email format'
				});
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
				privacy_consent: true,
				privacy_policy_version: 'test-v1'
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
						email_verified: true,
						role: 'user'
					})
			});

			const result = await fetchMe();

			expect(mockFetch).toHaveBeenCalledWith('/api/v1/auth/me', {
				credentials: 'same-origin'
			});
			expect(result.display_name).toBe('User');
		});
	});

	describe('OAuth and privacy', () => {
		it('loads provider availability and the current privacy version', async () => {
			mockFetch.mockResolvedValue({
				ok: true,
				json: () =>
					Promise.resolve({
						privacy_policy: { version: 'policy-v2', url: '/privacy' },
						oauth: { google: true, github: false }
					})
			});

			const options = await fetchAuthOptions();

			expect(mockFetch).toHaveBeenCalledWith('/api/v1/auth/options', {
				credentials: 'same-origin'
			});
			expect(options.privacy_policy.version).toBe('policy-v2');
			expect(options.oauth.github).toBe(false);
		});

		it('starts OAuth registration with the observed consent version', async () => {
			mockFetch.mockResolvedValue({
				ok: true,
				json: () => Promise.resolve({ authorization_url: 'https://provider.test/authorize' })
			});

			const url = await startOAuth('github', 'register', {
				privacy_consent: true,
				privacy_policy_version: 'policy-v2'
			});

			expect(url).toBe('https://provider.test/authorize');
			expect(mockFetch).toHaveBeenCalledWith('/api/v1/auth/oauth/github/start', {
				method: 'POST',
				headers: { 'Content-Type': 'application/json' },
				credentials: 'same-origin',
				body: JSON.stringify({
					intent: 'register',
					privacy_consent: true,
					privacy_policy_version: 'policy-v2'
				})
			});
		});

		it('starts OAuth login without inventing a new consent', async () => {
			mockFetch.mockResolvedValue({
				ok: true,
				json: () => Promise.resolve({ authorization_url: 'https://provider.test/login' })
			});

			await startOAuth('google', 'login');

			expect(JSON.parse(mockFetch.mock.calls[0][1].body)).toEqual({ intent: 'login' });
		});

		it('loads, confirms and cancels a pending OAuth link', async () => {
			mockFetch
				.mockResolvedValueOnce({
					ok: true,
					json: () =>
						Promise.resolve({
							pending: true,
							provider: 'google',
							masked_email: 'c***@example.com',
							confirmation_token: 'one-time-confirmation'
						})
				})
				.mockResolvedValueOnce({
					ok: true,
					json: () => Promise.resolve({ message: 'linked' })
				})
				.mockResolvedValueOnce({ ok: true, status: 204 });

			const pending = await fetchPendingOAuthLink();
			await confirmOAuthLink(pending.confirmation_token!);
			await cancelOAuthLink();

			expect(pending.masked_email).toBe('c***@example.com');
			expect(mockFetch).toHaveBeenNthCalledWith(1, '/api/v1/auth/oauth/link', {
				credentials: 'same-origin'
			});
			expect(mockFetch).toHaveBeenNthCalledWith(2, '/api/v1/auth/oauth/link', {
				method: 'POST',
				headers: {
					'Content-Type': 'application/json',
					'X-CSRF-Token': 'one-time-confirmation'
				},
				credentials: 'same-origin',
				body: '{}'
			});
			expect(mockFetch).toHaveBeenNthCalledWith(3, '/api/v1/auth/oauth/link', {
				method: 'DELETE',
				credentials: 'same-origin'
			});
		});

		it('surfaces a failed link cancellation', async () => {
			mockFetch.mockResolvedValue({
				ok: false,
				status: 409,
				json: () => Promise.resolve({ error: 'Link expired', code: 'OAUTH_LINK_REQUIRED' })
			});

			await expect(cancelOAuthLink()).rejects.toThrow('Link expired');
		});

		it('loads the private consent history', async () => {
			mockFetch.mockResolvedValue({
				ok: true,
				json: () =>
					Promise.resolve([
						{
							policy_version: 'policy-v1',
							consented_at: '2026-08-12T08:00:00',
							registration_method: 'github'
						}
					])
			});

			const consents = await fetchPrivacyConsents();

			expect(mockFetch).toHaveBeenCalledWith('/api/v1/me/privacy-consents', {
				credentials: 'same-origin'
			});
			expect(consents[0].registration_method).toBe('github');
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
			mockFetch.mockResolvedValue({
				ok: true,
				json: () => Promise.resolve({ message: 'ok' })
			});

			await resendVerification('user@test.com');

			expect(mockFetch).toHaveBeenCalledWith('/api/v1/auth/resend-verification', {
				method: 'POST',
				headers: { 'Content-Type': 'application/json' },
				credentials: 'same-origin',
				body: JSON.stringify({ email: 'user@test.com' })
			});
		});

		it('throws on server error', async () => {
			mockFetch.mockResolvedValue({
				ok: false,
				json: () => Promise.resolve({ error: 'Internal server error' })
			});

			await expect(resendVerification('user@test.com')).rejects.toThrow('Internal server error');
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
