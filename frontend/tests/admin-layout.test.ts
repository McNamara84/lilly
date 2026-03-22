import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/svelte';
import { createRawSnippet } from 'svelte';
import AdminLayout from '../src/routes/admin/+layout.svelte';

const mockGetAuthState = vi.fn();

vi.mock('$lib/stores/auth.svelte', () => ({
	getAuthState: () => mockGetAuthState()
}));

vi.mock('$app/navigation', () => ({
	goto: vi.fn()
}));

vi.mock('$app/paths', () => ({
	resolve: (path: string) => path
}));

import { goto } from '$app/navigation';

function renderAdminLayout() {
	const children = createRawSnippet(() => ({
		render: () => '<div data-testid="admin-child-content">Admin Content</div>'
	}));
	return render(AdminLayout, { props: { children } });
}

describe('Admin Layout', () => {
	beforeEach(() => {
		vi.clearAllMocks();
	});

	it('renders admin navigation and children when user is admin', () => {
		mockGetAuthState.mockReturnValue({
			isAuthenticated: true,
			isAdmin: true,
			isLoading: false,
			user: { id: 1, role: 'admin' }
		});

		renderAdminLayout();

		expect(screen.getByTestId('admin-nav-series')).toBeInTheDocument();
		expect(screen.getByTestId('admin-nav-import')).toBeInTheDocument();
		expect(screen.getByTestId('admin-child-content')).toBeInTheDocument();
	});

	it('has correct navigation links', () => {
		mockGetAuthState.mockReturnValue({
			isAuthenticated: true,
			isAdmin: true,
			isLoading: false,
			user: { id: 1, role: 'admin' }
		});

		renderAdminLayout();

		expect(screen.getByTestId('admin-nav-series')).toHaveAttribute('href', '/admin/series');
		expect(screen.getByTestId('admin-nav-import')).toHaveAttribute('href', '/admin/import');
	});

	it('has accessible navigation landmark', () => {
		mockGetAuthState.mockReturnValue({
			isAuthenticated: true,
			isAdmin: true,
			isLoading: false,
			user: { id: 1, role: 'admin' }
		});

		renderAdminLayout();

		expect(screen.getByRole('navigation', { name: 'Admin-Navigation' })).toBeInTheDocument();
	});

	it('redirects to login when not authenticated', async () => {
		mockGetAuthState.mockReturnValue({
			isAuthenticated: false,
			isAdmin: false,
			isLoading: false,
			user: null
		});

		renderAdminLayout();

		await waitFor(() => {
			expect(goto).toHaveBeenCalledWith('/login');
		});
	});

	it('redirects to home when authenticated but not admin', async () => {
		mockGetAuthState.mockReturnValue({
			isAuthenticated: true,
			isAdmin: false,
			isLoading: false,
			user: { id: 1, role: 'user' }
		});

		renderAdminLayout();

		await waitFor(() => {
			expect(goto).toHaveBeenCalledWith('/');
		});
	});

	it('does not redirect while loading', () => {
		mockGetAuthState.mockReturnValue({
			isAuthenticated: false,
			isAdmin: false,
			isLoading: true,
			user: null
		});

		renderAdminLayout();

		expect(goto).not.toHaveBeenCalled();
	});

	it('does not render children when not admin', () => {
		mockGetAuthState.mockReturnValue({
			isAuthenticated: true,
			isAdmin: false,
			isLoading: false,
			user: { id: 1, role: 'user' }
		});

		renderAdminLayout();

		expect(screen.queryByTestId('admin-child-content')).not.toBeInTheDocument();
	});
});
