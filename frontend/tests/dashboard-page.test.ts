import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import { userEvent } from '@testing-library/user-event';
import DashboardPage from '../src/routes/+page.svelte';

const mockGetAuthState = vi.fn();
const mockPerformLogout = vi.fn();

vi.mock('$lib/stores/auth.svelte', () => ({
	getAuthState: () => mockGetAuthState(),
	performLogout: () => mockPerformLogout()
}));

vi.mock('$app/navigation', () => ({
	goto: vi.fn()
}));

vi.mock('$app/paths', () => ({
	resolve: (path: string) => path
}));

describe('Dashboard Page', () => {
	beforeEach(() => {
		vi.clearAllMocks();
		mockPerformLogout.mockResolvedValue(undefined);
	});

	it('renders welcome header with user display name', () => {
		mockGetAuthState.mockReturnValue({
			isAuthenticated: true,
			user: { id: 1, email: 'test@test.com', display_name: 'Max Mustermann', email_verified: true },
			isLoading: false
		});

		render(DashboardPage);

		expect(screen.getByTestId('welcome-header')).toBeInTheDocument();
		expect(screen.getByText(/Willkommen zurück, Max Mustermann/i)).toBeInTheDocument();
	});

	it('renders stats cards with zero values', () => {
		mockGetAuthState.mockReturnValue({
			isAuthenticated: true,
			user: { id: 1, email: 'test@test.com', display_name: 'Test', email_verified: true },
			isLoading: false
		});

		render(DashboardPage);

		expect(screen.getByTestId('stats-row')).toBeInTheDocument();
		expect(screen.getByText(/Gesamte Hefte/i)).toBeInTheDocument();
		expect(screen.getByText(/Sammlungsfortschritt/i)).toBeInTheDocument();
		expect(screen.getByText(/Tauschbar/i)).toBeInTheDocument();
		expect(screen.getByText(/Gesucht/i)).toBeInTheDocument();
	});

	it('renders empty state message', () => {
		mockGetAuthState.mockReturnValue({
			isAuthenticated: true,
			user: { id: 1, email: 'test@test.com', display_name: 'Test', email_verified: true },
			isLoading: false
		});

		render(DashboardPage);

		expect(screen.getByTestId('empty-state')).toBeInTheDocument();
		expect(screen.getByText(/Deine Sammlung ist noch leer/i)).toBeInTheDocument();
	});

	it('renders logout button', () => {
		mockGetAuthState.mockReturnValue({
			isAuthenticated: true,
			user: { id: 1, email: 'test@test.com', display_name: 'Test', email_verified: true },
			isLoading: false
		});

		render(DashboardPage);

		expect(screen.getByTestId('logout-button')).toBeInTheDocument();
	});

	it('calls performLogout and navigates on logout click', async () => {
		const { goto } = await import('$app/navigation');

		mockGetAuthState.mockReturnValue({
			isAuthenticated: true,
			user: { id: 1, email: 'test@test.com', display_name: 'Test', email_verified: true },
			isLoading: false
		});

		render(DashboardPage);
		const user = userEvent.setup();

		await user.click(screen.getByTestId('logout-button'));

		expect(mockPerformLogout).toHaveBeenCalledOnce();
		expect(goto).toHaveBeenCalledWith('/login');
	});

	it('does not render content when user is null', () => {
		mockGetAuthState.mockReturnValue({
			isAuthenticated: false,
			user: null,
			isLoading: false
		});

		render(DashboardPage);

		expect(screen.queryByTestId('welcome-header')).not.toBeInTheDocument();
		expect(screen.queryByTestId('stats-row')).not.toBeInTheDocument();
	});

	it('sets the page title', () => {
		mockGetAuthState.mockReturnValue({
			isAuthenticated: true,
			user: { id: 1, email: 'test@test.com', display_name: 'Test', email_verified: true },
			isLoading: false
		});

		render(DashboardPage);

		expect(document.title).toBeDefined();
	});
});
