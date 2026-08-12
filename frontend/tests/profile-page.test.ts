import { beforeEach, describe, expect, it, vi } from 'vitest';
import { render, screen, waitFor } from '@testing-library/svelte';
import { userEvent } from '@testing-library/user-event';
import ProfilePage from '../src/routes/profile/+page.svelte';

const mockGetAuthState = vi.fn();
const mockFetchOwnProfile = vi.fn();
const mockUpdateVisibility = vi.fn();
const mockFetchPrivacyConsents = vi.fn();

vi.mock('$lib/stores/auth.svelte', () => ({
	getAuthState: () => mockGetAuthState()
}));

vi.mock('$lib/api/profile', () => ({
	fetchOwnProfile: (...args: unknown[]) => mockFetchOwnProfile(...args),
	updateVisibility: (...args: unknown[]) => mockUpdateVisibility(...args)
}));

vi.mock('$lib/api/auth', () => ({
	fetchPrivacyConsents: (...args: unknown[]) => mockFetchPrivacyConsents(...args)
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
		mockFetchPrivacyConsents.mockResolvedValue([
			{
				policy_version: 'test-v1',
				consented_at: '2026-01-01T00:00:00',
				registration_method: 'password'
			}
		]);
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
		expect(screen.getByTestId('privacy-consents-list')).toHaveTextContent('Version test-v1');
	});

	it('labels every supported registration method in consent history', async () => {
		mockFetchPrivacyConsents.mockResolvedValue([
			{
				policy_version: 'google-v1',
				consented_at: '2026-01-01T00:00:00',
				registration_method: 'google'
			},
			{
				policy_version: 'github-v1',
				consented_at: '2026-01-02T00:00:00',
				registration_method: 'github'
			},
			{
				policy_version: 'legacy-v1',
				consented_at: '2026-01-03T00:00:00',
				registration_method: 'legacy'
			}
		]);

		render(ProfilePage);

		const history = await screen.findByTestId('privacy-consents-list');
		expect(history).toHaveTextContent('Google');
		expect(history).toHaveTextContent('GitHub');
		expect(history).toHaveTextContent('Bestehendes Konto');
	});

	it('shows an explicit empty state when no versioned consent exists', async () => {
		mockFetchPrivacyConsents.mockResolvedValue([]);

		render(ProfilePage);

		expect(await screen.findByTestId('privacy-consents-empty')).toHaveTextContent(
			'noch kein versionierter Eintrag'
		);
	});

	it('keeps profile controls available when only consent history fails', async () => {
		mockFetchPrivacyConsents.mockRejectedValue(new Error('Consent-Historie nicht verfügbar'));

		render(ProfilePage);

		expect(await screen.findByTestId('profile-display-name')).toHaveTextContent('Sammler');
		expect(screen.getByTestId('save-visibility')).toBeEnabled();
		expect(screen.getByTestId('privacy-consents-error')).toHaveTextContent(
			'Consent-Historie nicht verfügbar'
		);
	});

	it('uses a scoped fallback for an untyped consent-history failure', async () => {
		mockFetchPrivacyConsents.mockRejectedValue('untyped consent failure');

		render(ProfilePage);

		expect(await screen.findByTestId('profile-display-name')).toBeInTheDocument();
		expect(screen.getByTestId('privacy-consents-error')).toHaveTextContent(
			'Datenschutz-Einwilligungen konnten nicht geladen werden.'
		);
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

	it('uses fallback messages for untyped load and update failures', async () => {
		mockFetchOwnProfile.mockRejectedValueOnce('untyped load failure');
		const failedLoad = render(ProfilePage);
		await waitFor(() =>
			expect(screen.getByRole('alert')).toHaveTextContent('Profil konnte nicht geladen werden.')
		);
		failedLoad.unmount();

		mockFetchOwnProfile.mockResolvedValueOnce({ ...profile });
		mockUpdateVisibility.mockRejectedValueOnce('untyped update failure');
		render(ProfilePage);
		const user = userEvent.setup();
		await waitFor(() => expect(screen.getByTestId('save-visibility')).toBeInTheDocument());
		await user.click(screen.getByTestId('save-visibility'));
		await waitFor(() =>
			expect(screen.getByRole('alert')).toHaveTextContent(
				'Sichtbarkeit konnte nicht gespeichert werden.'
			)
		);
	});

	it('redirects unauthenticated users to login without requesting profile data', async () => {
		const { goto } = await import('$app/navigation');
		mockGetAuthState.mockReturnValue({ isAuthenticated: false, isLoading: false, user: null });

		render(ProfilePage);

		await waitFor(() => expect(goto).toHaveBeenCalledWith('/login'));
		expect(mockFetchOwnProfile).not.toHaveBeenCalled();
	});

	it('waits for authentication to finish before loading or redirecting', async () => {
		const { goto } = await import('$app/navigation');
		mockGetAuthState.mockReturnValue({ isAuthenticated: false, isLoading: true, user: null });

		render(ProfilePage);

		expect(goto).not.toHaveBeenCalled();
		expect(mockFetchOwnProfile).not.toHaveBeenCalled();
		expect(screen.getByTestId('profile-loading')).toBeInTheDocument();
	});
});
