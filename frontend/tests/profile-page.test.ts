import { beforeEach, describe, expect, it, vi } from 'vitest';
import { render, screen, waitFor } from '@testing-library/svelte';
import { userEvent } from '@testing-library/user-event';
import ProfilePage from '../src/routes/profile/+page.svelte';

const mockGetAuthState = vi.fn();
const mockFetchOwnProfile = vi.fn();
const mockUpdateVisibility = vi.fn();

vi.mock('$lib/stores/auth.svelte', () => ({
	getAuthState: () => mockGetAuthState()
}));

vi.mock('$lib/api/profile', () => ({
	fetchOwnProfile: (...args: unknown[]) => mockFetchOwnProfile(...args),
	updateVisibility: (...args: unknown[]) => mockUpdateVisibility(...args)
}));

vi.mock('$app/navigation', () => ({ goto: vi.fn() }));
vi.mock('$app/paths', () => ({ resolve: (path: string) => path }));

const profile = {
	id: 7,
	email: 'sammler@example.com',
	display_name: 'Sammler',
	avatar_path: null,
	location: 'Berlin',
	profile_public: false,
	collection_public: false,
	created_at: '2026-01-01T00:00:00'
};

function authedState() {
	return {
		isAuthenticated: true,
		isLoading: false,
		user: {
			id: 7,
			email: profile.email,
			display_name: profile.display_name,
			email_verified: true,
			role: 'user' as const
		}
	};
}

describe('Profile Page', () => {
	beforeEach(() => {
		vi.clearAllMocks();
		mockGetAuthState.mockReturnValue(authedState());
		mockFetchOwnProfile.mockResolvedValue({ ...profile });
	});

	it('loads private account data and initializes both visibility toggles', async () => {
		render(ProfilePage);

		await waitFor(() =>
			expect(screen.getByTestId('profile-display-name')).toHaveTextContent('Sammler')
		);
		expect(screen.getByTestId('profile-email')).toHaveTextContent('sammler@example.com');
		expect(screen.getByTestId('profile-public-toggle')).not.toBeChecked();
		expect(screen.getByTestId('collection-public-toggle')).not.toBeChecked();
		expect(mockFetchOwnProfile).toHaveBeenCalledOnce();
	});

	it('shows the explicit warning that public collections include personal notes', async () => {
		render(ProfilePage);

		await waitFor(() => expect(screen.getByTestId('collection-public-toggle')).toBeInTheDocument());
		expect(
			screen.getByText('Öffentliche Sammlungen zeigen auch deine persönlichen Heftnotizen.')
		).toBeInTheDocument();
		expect(screen.getByTestId('collection-public-toggle')).toHaveAttribute(
			'aria-describedby',
			'collection-public-warning'
		);
	});

	it('saves every combination without coupling profile and collection visibility', async () => {
		mockUpdateVisibility.mockResolvedValue({
			profile_public: false,
			collection_public: true
		});
		render(ProfilePage);
		const user = userEvent.setup();

		await waitFor(() => expect(screen.getByTestId('collection-public-toggle')).toBeInTheDocument());
		await user.click(screen.getByTestId('collection-public-toggle'));
		await user.click(screen.getByTestId('save-visibility'));

		await waitFor(() =>
			expect(mockUpdateVisibility).toHaveBeenCalledWith({
				profile_public: false,
				collection_public: true
			})
		);
		expect(screen.getByTestId('profile-success')).toHaveTextContent('Sichtbarkeit gespeichert.');
		expect(screen.getByRole('link', { name: 'Öffentliche Sammlung ansehen' })).toHaveAttribute(
			'href',
			'/users/7/collection'
		);
	});

	it('sends a profile-only public configuration independently', async () => {
		mockUpdateVisibility.mockResolvedValue({
			profile_public: true,
			collection_public: false
		});
		render(ProfilePage);
		const user = userEvent.setup();

		await waitFor(() => expect(screen.getByTestId('profile-public-toggle')).toBeInTheDocument());
		await user.click(screen.getByTestId('profile-public-toggle'));
		await user.click(screen.getByTestId('save-visibility'));

		await waitFor(() =>
			expect(mockUpdateVisibility).toHaveBeenCalledWith({
				profile_public: true,
				collection_public: false
			})
		);
		expect(
			screen.queryByRole('link', { name: 'Öffentliche Sammlung ansehen' })
		).not.toBeInTheDocument();
	});

	it('reports load and update failures accessibly', async () => {
		mockFetchOwnProfile.mockRejectedValueOnce(new Error('Profilfehler'));
		const failedLoad = render(ProfilePage);
		await waitFor(() => expect(screen.getByRole('alert')).toHaveTextContent('Profilfehler'));
		failedLoad.unmount();

		mockFetchOwnProfile.mockResolvedValueOnce({ ...profile });
		mockUpdateVisibility.mockRejectedValueOnce(new Error('Speicherfehler'));
		render(ProfilePage);
		const user = userEvent.setup();
		await waitFor(() => expect(screen.getByTestId('save-visibility')).toBeInTheDocument());
		await user.click(screen.getByTestId('save-visibility'));
		await waitFor(() => expect(screen.getByRole('alert')).toHaveTextContent('Speicherfehler'));
	});

	it('redirects unauthenticated users to login without requesting profile data', async () => {
		const { goto } = await import('$app/navigation');
		mockGetAuthState.mockReturnValue({ isAuthenticated: false, isLoading: false, user: null });

		render(ProfilePage);

		await waitFor(() => expect(goto).toHaveBeenCalledWith('/login'));
		expect(mockFetchOwnProfile).not.toHaveBeenCalled();
	});
});
