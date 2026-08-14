import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { userEvent } from '@testing-library/user-event';
import ProfilePage from '../src/routes/profile/+page.svelte';

const mockGetAuthState = vi.fn();
const mockFetchOwnProfile = vi.fn();
const mockUpdateProfile = vi.fn();
const mockUpdateVisibility = vi.fn();
const mockUploadAvatar = vi.fn();
const mockDeleteAvatar = vi.fn();
const mockFetchPrivacyConsents = vi.fn();
const mockFetchPhotoPolicy = vi.fn();
const mockSetUser = vi.fn();

vi.mock('$lib/stores/auth.svelte', () => ({
	getAuthState: () => mockGetAuthState(),
	setUser: (...args: unknown[]) => mockSetUser(...args)
}));

vi.mock('$lib/api/profile', () => ({
	fetchOwnProfile: (...args: unknown[]) => mockFetchOwnProfile(...args),
	updateProfile: (...args: unknown[]) => mockUpdateProfile(...args),
	uploadAvatar: (...args: unknown[]) => mockUploadAvatar(...args),
	deleteAvatar: (...args: unknown[]) => mockDeleteAvatar(...args),
	updateVisibility: (...args: unknown[]) => mockUpdateVisibility(...args)
}));

vi.mock('$lib/api/media', () => ({
	DEFAULT_PHOTO_POLICY: {
		allowed_media_types: ['image/jpeg', 'image/png', 'image/webp'],
		max_upload_bytes: 5 * 1024 * 1024,
		max_photos: 4,
		max_edge: 2048
	},
	fetchPhotoPolicy: (...args: unknown[]) => mockFetchPhotoPolicy(...args)
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
	avatar_url: null,
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
		mockUpdateProfile.mockResolvedValue({ ...profile });
		mockUploadAvatar.mockResolvedValue({
			...profile,
			avatar_url: '/api/v1/users/7/avatar'
		});
		mockDeleteAvatar.mockResolvedValue(undefined);
		mockFetchPhotoPolicy.mockResolvedValue({
			allowed_media_types: ['image/jpeg', 'image/png', 'image/webp'],
			max_upload_bytes: 5 * 1024 * 1024,
			max_photos: 4,
			max_edge: 2048
		});
		mockFetchPrivacyConsents.mockResolvedValue([
			{
				policy_version: 'test-v1',
				consented_at: '2026-01-01T00:00:00',
				registration_method: 'password'
			}
		]);
	});

	it('edits and normalizes display name and optional location', async () => {
		mockUpdateProfile.mockResolvedValue({
			...profile,
			display_name: 'Neue Sammlerin',
			location: null
		});
		render(ProfilePage);
		const user = userEvent.setup();

		const name = await screen.findByTestId('profile-display-name-input');
		await user.clear(name);
		await user.type(name, '  Neue Sammlerin  ');
		const location = screen.getByTestId('profile-location-input');
		await user.clear(location);
		await user.type(location, '   ');
		await user.click(screen.getByTestId('save-profile'));

		await waitFor(() =>
			expect(mockUpdateProfile).toHaveBeenCalledWith({
				display_name: 'Neue Sammlerin',
				location: null
			})
		);
		expect(mockSetUser).toHaveBeenCalledWith(
			expect.objectContaining({ display_name: 'Neue Sammlerin' })
		);
		expect(screen.getByTestId('profile-success')).toHaveTextContent('Profildaten gespeichert.');
	});

	it('validates editable fields before sending them', async () => {
		render(ProfilePage);
		const user = userEvent.setup();
		const name = await screen.findByTestId('profile-display-name-input');
		await user.clear(name);
		await user.type(name, 'X');
		await user.click(screen.getByTestId('save-profile'));

		expect(
			await screen.findByText('Der Anzeigename muss 2 bis 100 Zeichen lang sein.')
		).toBeVisible();
		expect(name).toHaveAttribute('aria-invalid', 'true');
		expect(mockUpdateProfile).not.toHaveBeenCalled();
	});

	it('validates the Unicode length of the optional location', async () => {
		render(ProfilePage);
		const user = userEvent.setup();
		const location = await screen.findByTestId('profile-location-input');
		await fireEvent.input(location, { target: { value: '📚'.repeat(256) } });
		await user.click(screen.getByTestId('save-profile'));

		expect(
			await screen.findByText('Der Standort darf höchstens 255 Zeichen lang sein.')
		).toBeVisible();
		expect(location).toHaveAttribute('aria-invalid', 'true');
		expect(location).toHaveAttribute('aria-describedby', 'location-hint location-error');
		expect(mockUpdateProfile).not.toHaveBeenCalled();
	});

	it('maps backend field validation errors back to the corresponding input', async () => {
		mockUpdateProfile.mockRejectedValue(
			Object.assign(new Error('Validierung fehlgeschlagen'), {
				fields: { location: 'Dieser Standort ist nicht zulässig.' }
			})
		);
		render(ProfilePage);
		const user = userEvent.setup();

		await screen.findByTestId('profile-location-input');
		await user.click(screen.getByTestId('save-profile'));

		expect(await screen.findByText('Dieser Standort ist nicht zulässig.')).toBeVisible();
		expect(screen.getByTestId('profile-location-input')).toHaveAttribute('aria-invalid', 'true');
		expect(screen.getByRole('alert')).toHaveTextContent('Validierung fehlgeschlagen');
	});

	it('uploads and removes an avatar with accessible controls', async () => {
		render(ProfilePage);
		const user = userEvent.setup();
		const input = await screen.findByTestId('profile-avatar-input');
		const avatar = new File(['avatar'], 'avatar.png', { type: 'image/png' });

		await user.upload(input, avatar);
		await waitFor(() => expect(mockUploadAvatar).toHaveBeenCalledWith(avatar));
		expect(await screen.findByAltText('Avatar von Sammler')).toHaveAttribute(
			'src',
			expect.stringContaining('/api/v1/users/7/avatar?v=')
		);
		await user.click(screen.getByTestId('delete-avatar'));
		await waitFor(() => expect(mockDeleteAvatar).toHaveBeenCalledOnce());
		expect(screen.queryByTestId('delete-avatar')).not.toBeInTheDocument();
		expect(screen.getByTestId('profile-success')).toHaveTextContent('Avatar entfernt.');
	});

	it('rejects unsupported or oversized avatars before upload', async () => {
		render(ProfilePage);
		const user = userEvent.setup({ applyAccept: false });
		const input = await screen.findByTestId('profile-avatar-input');

		await user.upload(input, new File(['text'], 'avatar.txt', { type: 'text/plain' }));
		expect(await screen.findByTestId('profile-error')).toHaveTextContent('JPEG-, PNG- oder WebP');
		expect(mockUploadAvatar).not.toHaveBeenCalled();

		await user.upload(
			input,
			new File([new Uint8Array(5 * 1024 * 1024 + 1)], 'large.png', { type: 'image/png' })
		);
		await waitFor(() => expect(screen.getByTestId('profile-error')).toHaveTextContent('höchstens'));
		expect(mockUploadAvatar).not.toHaveBeenCalled();
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
