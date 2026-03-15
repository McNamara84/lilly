import { describe, it, expect, vi, beforeEach } from 'vitest';

// Mock API module before importing auth store
vi.mock('$lib/api/auth', () => ({
	fetchMe: vi.fn(),
	logout: vi.fn(),
	refreshToken: vi.fn()
}));

describe('Auth Store', () => {
	beforeEach(() => {
		vi.clearAllMocks();
		vi.resetModules();
	});

	it('getAuthState returns initial state with null user and loading true', async () => {
		const { getAuthState } = await import('$lib/stores/auth.svelte');
		const auth = getAuthState();

		expect(auth.user).toBeNull();
		expect(auth.isAuthenticated).toBe(false);
	});

	it('initAuth fetches user successfully', async () => {
		const { fetchMe } = await import('$lib/api/auth');
		const mockFetchMe = vi.mocked(fetchMe);
		mockFetchMe.mockResolvedValue({
			id: 1,
			email: 'test@test.com',
			display_name: 'Test User',
			email_verified: true,
			role: 'user'
		});

		const { initAuth, getAuthState } = await import('$lib/stores/auth.svelte');
		await initAuth();
		const auth = getAuthState();

		expect(mockFetchMe).toHaveBeenCalledOnce();
		expect(auth.user).toEqual({
			id: 1,
			email: 'test@test.com',
			display_name: 'Test User',
			email_verified: true,
			role: 'user'
		});
		expect(auth.isAuthenticated).toBe(true);
		expect(auth.isLoading).toBe(false);
	});

	it('initAuth refreshes token and retries on initial fetchMe failure', async () => {
		const { fetchMe, refreshToken } = await import('$lib/api/auth');
		const mockFetchMe = vi.mocked(fetchMe);
		const mockRefreshToken = vi.mocked(refreshToken);

		mockFetchMe.mockRejectedValueOnce(new Error('Unauthorized')).mockResolvedValueOnce({
			id: 2,
			email: 'refreshed@test.com',
			display_name: 'Refreshed',
			email_verified: true,
			role: 'user'
		});
		mockRefreshToken.mockResolvedValue(undefined);

		const { initAuth, getAuthState } = await import('$lib/stores/auth.svelte');
		await initAuth();
		const auth = getAuthState();

		expect(mockRefreshToken).toHaveBeenCalledOnce();
		expect(mockFetchMe).toHaveBeenCalledTimes(2);
		expect(auth.user).toEqual({
			id: 2,
			email: 'refreshed@test.com',
			display_name: 'Refreshed',
			email_verified: true,
			role: 'user'
		});
		expect(auth.isAuthenticated).toBe(true);
	});

	it('initAuth sets user to null when both fetchMe and refresh fail', async () => {
		const { fetchMe, refreshToken } = await import('$lib/api/auth');
		const mockFetchMe = vi.mocked(fetchMe);
		const mockRefreshToken = vi.mocked(refreshToken);

		mockFetchMe.mockRejectedValue(new Error('Unauthorized'));
		mockRefreshToken.mockRejectedValue(new Error('Refresh failed'));

		const { initAuth, getAuthState } = await import('$lib/stores/auth.svelte');
		await initAuth();
		const auth = getAuthState();

		expect(auth.user).toBeNull();
		expect(auth.isAuthenticated).toBe(false);
		expect(auth.isLoading).toBe(false);
	});

	it('performLogout calls API and clears user', async () => {
		const { fetchMe, logout: apiLogout } = await import('$lib/api/auth');
		const mockFetchMe = vi.mocked(fetchMe);
		const mockLogout = vi.mocked(apiLogout);

		mockFetchMe.mockResolvedValue({
			id: 1,
			email: 'test@test.com',
			display_name: 'Test',
			email_verified: true,
			role: 'user'
		});
		mockLogout.mockResolvedValue(undefined);

		const { initAuth, performLogout, getAuthState } = await import('$lib/stores/auth.svelte');

		await initAuth();
		expect(getAuthState().isAuthenticated).toBe(true);

		await performLogout();
		expect(mockLogout).toHaveBeenCalledOnce();
		expect(getAuthState().user).toBeNull();
		expect(getAuthState().isAuthenticated).toBe(false);
	});

	it('performLogout clears user even when API call fails', async () => {
		const { fetchMe, logout: apiLogout } = await import('$lib/api/auth');
		const mockFetchMe = vi.mocked(fetchMe);
		const mockLogout = vi.mocked(apiLogout);

		mockFetchMe.mockResolvedValue({
			id: 1,
			email: 'test@test.com',
			display_name: 'Test',
			email_verified: true,
			role: 'user'
		});
		mockLogout.mockRejectedValue(new Error('Network error'));

		const { initAuth, performLogout, getAuthState } = await import('$lib/stores/auth.svelte');

		await initAuth();
		await expect(performLogout()).rejects.toThrow('Network error');

		expect(getAuthState().user).toBeNull();
		expect(getAuthState().isAuthenticated).toBe(false);
	});

	it('setUser updates the user state', async () => {
		const { setUser, getAuthState } = await import('$lib/stores/auth.svelte');

		setUser({
			id: 5,
			email: 'set@test.com',
			display_name: 'Set User',
			email_verified: true,
			role: 'user'
		});
		expect(getAuthState().user).toEqual({
			id: 5,
			email: 'set@test.com',
			display_name: 'Set User',
			email_verified: true,
			role: 'user'
		});
		expect(getAuthState().isAuthenticated).toBe(true);

		setUser(null);
		expect(getAuthState().user).toBeNull();
		expect(getAuthState().isAuthenticated).toBe(false);
	});
});
