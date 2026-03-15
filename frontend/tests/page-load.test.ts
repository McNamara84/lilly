import { describe, it, expect, vi, beforeEach } from 'vitest';

const mockGetAuthState = vi.fn();

vi.mock('$lib/stores/auth.svelte', () => ({
	getAuthState: () => mockGetAuthState()
}));

// Mock @sveltejs/kit redirect as a thrown object
vi.mock('@sveltejs/kit', () => ({
	redirect: (status: number, location: string) => {
		throw { status, location };
	}
}));

describe('+page.ts load function', () => {
	beforeEach(() => {
		vi.clearAllMocks();
		vi.resetModules();
	});

	it('redirects to /login when not authenticated and not loading', async () => {
		mockGetAuthState.mockReturnValue({
			isAuthenticated: false,
			isLoading: false,
			user: null
		});

		const { load } = await import('../src/routes/+page.ts');

		expect(() => load()).toThrow(expect.objectContaining({ status: 302, location: '/login' }));
	});

	it('does not redirect when authenticated', async () => {
		mockGetAuthState.mockReturnValue({
			isAuthenticated: true,
			isLoading: false,
			user: {
				id: 1,
				email: 'test@test.com',
				display_name: 'Test',
				email_verified: true,
				role: 'user' as const
			}
		});

		const { load } = await import('../src/routes/+page.ts');

		expect(() => load()).not.toThrow();
	});

	it('does not redirect when still loading', async () => {
		mockGetAuthState.mockReturnValue({
			isAuthenticated: false,
			isLoading: true,
			user: null
		});

		const { load } = await import('../src/routes/+page.ts');

		expect(() => load()).not.toThrow();
	});
});
