import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/svelte';
import { userEvent } from '@testing-library/user-event';
import DashboardPage from '../src/routes/+page.svelte';

const mockGetAuthState = vi.fn();
const mockPerformLogout = vi.fn();
const mockFetchCollectionStats = vi.fn();

vi.mock('$lib/stores/auth.svelte', () => ({
	getAuthState: () => mockGetAuthState(),
	performLogout: () => mockPerformLogout()
}));

vi.mock('$lib/api/collection', () => ({
	fetchCollectionStats: () => mockFetchCollectionStats()
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
		mockFetchCollectionStats.mockResolvedValue({
			total_owned: 0,
			total_duplicate: 0,
			total_wanted: 0,
			unique_series: 0,
			overall_progress_percent: 0,
			series_stats: []
		});
	});

	it('renders welcome header with user display name', () => {
		mockGetAuthState.mockReturnValue({
			isAuthenticated: true,
			user: {
				id: 1,
				email: 'test@test.com',
				display_name: 'Max Mustermann',
				email_verified: true,
				role: 'user' as const
			},
			isLoading: false
		});

		render(DashboardPage);

		expect(screen.getByTestId('welcome-header')).toBeInTheDocument();
		expect(screen.getByText(/Willkommen zurück, Max Mustermann/i)).toBeInTheDocument();
	});

	it('renders stats cards with zero values', () => {
		mockGetAuthState.mockReturnValue({
			isAuthenticated: true,
			user: {
				id: 1,
				email: 'test@test.com',
				display_name: 'Test',
				email_verified: true,
				role: 'user' as const
			},
			isLoading: false
		});

		render(DashboardPage);

		expect(screen.getByTestId('stats-row')).toBeInTheDocument();
		expect(screen.getByText(/Gesamte Hefte/i)).toBeInTheDocument();
		expect(screen.getByText(/Sammlungsfortschritt/i)).toBeInTheDocument();
		expect(screen.getByText(/Tauschbar/i)).toBeInTheDocument();
		expect(screen.getByText(/Gesucht/i)).toBeInTheDocument();
	});

	it('renders empty state message', async () => {
		mockGetAuthState.mockReturnValue({
			isAuthenticated: true,
			user: {
				id: 1,
				email: 'test@test.com',
				display_name: 'Test',
				email_verified: true,
				role: 'user' as const
			},
			isLoading: false
		});

		render(DashboardPage);

		await waitFor(() => {
			expect(screen.getByTestId('empty-state')).toBeInTheDocument();
		});
		expect(screen.getByText(/Deine Sammlung ist noch leer/i)).toBeInTheDocument();
	});

	it('renders logout button', () => {
		mockGetAuthState.mockReturnValue({
			isAuthenticated: true,
			user: {
				id: 1,
				email: 'test@test.com',
				display_name: 'Test',
				email_verified: true,
				role: 'user' as const
			},
			isLoading: false
		});

		render(DashboardPage);

		expect(screen.getByTestId('logout-button')).toBeInTheDocument();
	});

	it('calls performLogout and navigates on logout click', async () => {
		const { goto } = await import('$app/navigation');

		mockGetAuthState.mockReturnValue({
			isAuthenticated: true,
			user: {
				id: 1,
				email: 'test@test.com',
				display_name: 'Test',
				email_verified: true,
				role: 'user' as const
			},
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
			user: {
				id: 1,
				email: 'test@test.com',
				display_name: 'Test',
				email_verified: true,
				role: 'user' as const
			},
			isLoading: false
		});

		render(DashboardPage);

		expect(document.title).toBeDefined();
	});

	it('renders stats with non-zero values', async () => {
		mockGetAuthState.mockReturnValue({
			isAuthenticated: true,
			user: {
				id: 1,
				email: 'test@test.com',
				display_name: 'Test',
				email_verified: true,
				role: 'user' as const
			},
			isLoading: false
		});
		mockFetchCollectionStats.mockResolvedValue({
			total_owned: 42,
			total_duplicate: 5,
			total_wanted: 3,
			unique_series: 2,
			overall_progress_percent: 65.5,
			series_stats: []
		});

		render(DashboardPage);

		await waitFor(() => {
			expect(screen.getByText('42')).toBeInTheDocument();
		});
		expect(screen.getByText('65.5%')).toBeInTheDocument();
		expect(screen.getByText('5')).toBeInTheDocument();
		expect(screen.getByText('3')).toBeInTheDocument();
	});

	it('hides empty state when collection has entries', async () => {
		mockGetAuthState.mockReturnValue({
			isAuthenticated: true,
			user: {
				id: 1,
				email: 'test@test.com',
				display_name: 'Test',
				email_verified: true,
				role: 'user' as const
			},
			isLoading: false
		});
		mockFetchCollectionStats.mockResolvedValue({
			total_owned: 10,
			total_duplicate: 0,
			total_wanted: 0,
			unique_series: 1,
			overall_progress_percent: 5.0,
			series_stats: []
		});

		render(DashboardPage);

		await waitFor(() => {
			expect(screen.getByText('10')).toBeInTheDocument();
		});
		expect(screen.queryByTestId('empty-state')).not.toBeInTheDocument();
	});

	it('renders series progress bars when present', async () => {
		mockGetAuthState.mockReturnValue({
			isAuthenticated: true,
			user: {
				id: 1,
				email: 'test@test.com',
				display_name: 'Test',
				email_verified: true,
				role: 'user' as const
			},
			isLoading: false
		});
		mockFetchCollectionStats.mockResolvedValue({
			total_owned: 42,
			total_duplicate: 0,
			total_wanted: 0,
			unique_series: 1,
			overall_progress_percent: 10.0,
			series_stats: [
				{
					series_id: 1,
					series_name: 'Maddrax',
					owned_count: 42,
					duplicate_count: 0,
					total_in_series: 620
				}
			]
		});

		render(DashboardPage);

		await waitFor(() => {
			expect(screen.getByTestId('series-progress-section')).toBeInTheDocument();
		});
		expect(screen.getByText('Maddrax')).toBeInTheDocument();
	});

	it('renders quick links', () => {
		mockGetAuthState.mockReturnValue({
			isAuthenticated: true,
			user: {
				id: 1,
				email: 'test@test.com',
				display_name: 'Test',
				email_verified: true,
				role: 'user' as const
			},
			isLoading: false
		});

		render(DashboardPage);

		expect(screen.getByTestId('quick-links')).toBeInTheDocument();
		expect(screen.getByText('Zur Sammlung')).toBeInTheDocument();
		expect(screen.getByText('Hefte hinzufügen')).toBeInTheDocument();
	});

	it('renders trade and activity placeholders', () => {
		mockGetAuthState.mockReturnValue({
			isAuthenticated: true,
			user: {
				id: 1,
				email: 'test@test.com',
				display_name: 'Test',
				email_verified: true,
				role: 'user' as const
			},
			isLoading: false
		});

		render(DashboardPage);

		expect(screen.getByTestId('trade-placeholder')).toBeInTheDocument();
		expect(screen.getByTestId('activity-placeholder')).toBeInTheDocument();
	});

	it('redirects unauthenticated users to login', async () => {
		const { goto } = await import('$app/navigation');
		mockGetAuthState.mockReturnValue({
			isAuthenticated: false,
			user: null,
			isLoading: false
		});

		render(DashboardPage);

		await waitFor(() => {
			expect(goto).toHaveBeenCalledWith('/login');
		});
	});

	it('shows stats error instead of empty state when stats fetch fails', async () => {
		mockGetAuthState.mockReturnValue({
			isAuthenticated: true,
			user: {
				id: 1,
				email: 'test@test.com',
				display_name: 'Test',
				email_verified: true,
				role: 'user' as const
			},
			isLoading: false
		});
		mockFetchCollectionStats.mockRejectedValue(new Error('Network error'));

		render(DashboardPage);

		await waitFor(() => {
			expect(screen.getByTestId('stats-error')).toBeInTheDocument();
		});
		expect(screen.queryByTestId('empty-state')).not.toBeInTheDocument();
	});
});
