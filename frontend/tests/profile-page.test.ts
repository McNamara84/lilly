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
const mockDeactivateAccountLocally = vi.fn();
const mockFetchDeletionOptions = vi.fn();
const mockRequestAccountDeletion = vi.fn();
const mockReauthenticateWithPassword = vi.fn();
const mockGoto = vi.fn();
const mockOfflineStatus = { online: true };

vi.mock('$lib/stores/auth.svelte', () => ({
	getAuthState: () => mockGetAuthState(),
	setUser: (...args: unknown[]) => mockSetUser(...args),
	deactivateAccountLocally: (...args: unknown[]) => mockDeactivateAccountLocally(...args)
}));

vi.mock('$lib/offline/status.svelte', () => ({
	getOfflineStatus: () => mockOfflineStatus
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
	fetchPrivacyConsents: (...args: unknown[]) => mockFetchPrivacyConsents(...args),
	startOAuth: vi.fn()
}));

vi.mock('$lib/api/account-erasure', () => ({
	fetchAccountDeletionOptions: (...args: unknown[]) => mockFetchDeletionOptions(...args),
	requestAccountDeletion: (...args: unknown[]) => mockRequestAccountDeletion(...args),
	reauthenticateWithPassword: (...args: unknown[]) => mockReauthenticateWithPassword(...args),
	availableOAuthMethods: (options: { google: boolean; github: boolean }) =>
		(['google', 'github'] as const).filter((provider) => options[provider])
}));

vi.mock('$app/navigation', () => ({ goto: vi.fn((...args: unknown[]) => mockGoto(...args)) }));
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
		mockOfflineStatus.online = true;
		window.history.replaceState(null, '', '/');
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
		mockFetchDeletionOptions.mockResolvedValue({
			recent_authentication: true,
			password: true,
			google: false,
			github: false,
			confirmation_phrase: 'KONTO LÖSCHEN',
			grace_days: 7
		});
		mockRequestAccountDeletion.mockResolvedValue({ status: 'scheduled' });
		mockDeactivateAccountLocally.mockResolvedValue(undefined);
		HTMLDialogElement.prototype.showModal = function () {
			this.setAttribute('open', '');
		};
		HTMLDialogElement.prototype.close = function () {
			this.removeAttribute('open');
		};
	});

	it('requires the exact phrase and purges local data after scheduling deletion', async () => {
		render(ProfilePage);
		const user = userEvent.setup();

		await user.click(await screen.findByTestId('open-account-deletion'));
		const confirmation = screen.getByTestId('account-deletion-confirmation');
		const submit = screen.getByTestId('confirm-account-deletion');
		expect(submit).toBeDisabled();
		await user.type(confirmation, 'KONTO LÖSCHEN');
		expect(submit).toBeEnabled();
		await user.click(submit);

		await waitFor(() => expect(mockRequestAccountDeletion).toHaveBeenCalledWith('KONTO LÖSCHEN'));
		expect(mockReauthenticateWithPassword).not.toHaveBeenCalled();
		expect(mockDeactivateAccountLocally).toHaveBeenCalledOnce();
		expect(mockGoto).toHaveBeenCalledWith('/account/deletion');
	});

	it('refreshes reauthentication choices when recent authentication expires in the dialog', async () => {
		const recentAuthError = Object.assign(new Error('Recent authentication required'), {
			code: 'RECENT_AUTH_REQUIRED'
		});
		mockRequestAccountDeletion.mockRejectedValueOnce(recentAuthError);
		mockFetchDeletionOptions
			.mockResolvedValueOnce({
				recent_authentication: true,
				password: true,
				google: false,
				github: false,
				confirmation_phrase: 'KONTO LÖSCHEN',
				grace_days: 7
			})
			.mockResolvedValueOnce({
				recent_authentication: false,
				password: true,
				google: false,
				github: false,
				confirmation_phrase: 'KONTO LÖSCHEN',
				grace_days: 7
			});
		render(ProfilePage);
		const user = userEvent.setup();

		await user.click(await screen.findByTestId('open-account-deletion'));
		await user.type(screen.getByTestId('account-deletion-confirmation'), 'KONTO LÖSCHEN');
		await user.click(screen.getByTestId('confirm-account-deletion'));

		expect(await screen.findByText('Anmeldung erneut bestätigen')).toBeInTheDocument();
		expect(screen.getByLabelText('Passwort')).toBeInTheDocument();
		expect(screen.getByRole('alert')).toHaveTextContent(/Bitte bestätige deine Anmeldung erneut/);
	});

	it('reauthenticates with the password before scheduling deletion', async () => {
		mockFetchDeletionOptions.mockResolvedValue({
			recent_authentication: false,
			password: true,
			google: false,
			github: false,
			confirmation_phrase: 'KONTO LÖSCHEN',
			grace_days: 7
		});
		render(ProfilePage);
		const user = userEvent.setup();

		await user.click(await screen.findByTestId('open-account-deletion'));
		await user.type(screen.getByLabelText('Passwort'), 'very secret');
		await user.type(screen.getByTestId('account-deletion-confirmation'), 'KONTO LÖSCHEN');
		await user.click(screen.getByTestId('confirm-account-deletion'));

		await waitFor(() => expect(mockReauthenticateWithPassword).toHaveBeenCalledWith('very secret'));
		expect(mockReauthenticateWithPassword.mock.invocationCallOrder[0]).toBeLessThan(
			mockRequestAccountDeletion.mock.invocationCallOrder[0]
		);
		expect(mockDeactivateAccountLocally).toHaveBeenCalledOnce();
	});

	it('starts OAuth reauthentication with a linked provider', async () => {
		const { startOAuth } = await import('$lib/api/auth');
		vi.mocked(startOAuth).mockResolvedValue('#github-reauth');
		mockFetchDeletionOptions.mockResolvedValue({
			recent_authentication: false,
			password: false,
			google: false,
			github: true,
			confirmation_phrase: 'KONTO LÖSCHEN',
			grace_days: 7
		});
		render(ProfilePage);
		const user = userEvent.setup();

		await user.click(await screen.findByTestId('open-account-deletion'));
		await user.click(screen.getByRole('button', { name: 'Mit GitHub bestätigen' }));

		expect(startOAuth).toHaveBeenCalledWith('github', 'reauth');
		await waitFor(() => expect(window.location.hash).toBe('#github-reauth'));
	});

	it('reports OAuth reauthentication startup failures and resets the button', async () => {
		const { startOAuth } = await import('$lib/api/auth');
		vi.mocked(startOAuth).mockRejectedValue('untyped OAuth failure');
		mockFetchDeletionOptions.mockResolvedValue({
			recent_authentication: false,
			password: false,
			google: true,
			github: false,
			confirmation_phrase: 'KONTO LÖSCHEN',
			grace_days: 7
		});
		render(ProfilePage);
		const user = userEvent.setup();

		await user.click(await screen.findByTestId('open-account-deletion'));
		const oauthButton = screen.getByRole('button', { name: 'Mit Google bestätigen' });
		await user.click(oauthButton);

		expect(await screen.findByRole('alert')).toHaveTextContent(
			'Anmeldung konnte nicht gestartet werden.'
		);
		expect(oauthButton).toBeEnabled();
	});

	it('requires a linked provider when password reauthentication is unavailable', async () => {
		mockFetchDeletionOptions.mockResolvedValue({
			recent_authentication: false,
			password: false,
			google: false,
			github: false,
			confirmation_phrase: 'KONTO LÖSCHEN',
			grace_days: 7
		});
		render(ProfilePage);
		const user = userEvent.setup();

		await user.click(await screen.findByTestId('open-account-deletion'));
		await user.type(screen.getByTestId('account-deletion-confirmation'), 'KONTO LÖSCHEN');
		await user.click(screen.getByTestId('confirm-account-deletion'));

		expect(await screen.findByRole('alert')).toHaveTextContent(/verknüpften Anbieter/);
		expect(mockRequestAccountDeletion).not.toHaveBeenCalled();
		await fireEvent(document.querySelector('dialog')!, new Event('close'));
		expect(screen.queryByRole('alert')).not.toBeInTheDocument();
	});

	it('disables account deletion for an offline session', async () => {
		mockGetAuthState.mockReturnValue({ ...authedState(), isOfflineSession: true });
		render(ProfilePage);

		expect(await screen.findByTestId('open-account-deletion')).toBeDisabled();
		expect(screen.getByText('Diese Aktion ist offline nicht verfügbar.')).toBeInTheDocument();
	});

	it('uses the shared connectivity state to disable account deletion', async () => {
		mockOfflineStatus.online = false;
		render(ProfilePage);

		expect(await screen.findByTestId('open-account-deletion')).toBeDisabled();
		expect(screen.getByText('Diese Aktion ist offline nicht verfügbar.')).toBeInTheDocument();
	});

	it('shows deletion-option failures and retries loading them', async () => {
		mockFetchDeletionOptions
			.mockRejectedValueOnce(new Error('Löschoptionen sind vorübergehend nicht verfügbar.'))
			.mockResolvedValueOnce({
				recent_authentication: true,
				password: true,
				google: false,
				github: false,
				confirmation_phrase: 'KONTO LÖSCHEN',
				grace_days: 7
			});
		render(ProfilePage);
		const user = userEvent.setup();

		expect(await screen.findByTestId('account-deletion-options-error')).toHaveTextContent(
			'Löschoptionen sind vorübergehend nicht verfügbar.'
		);
		expect(screen.getByTestId('open-account-deletion')).toBeDisabled();
		await user.click(screen.getByTestId('retry-account-deletion-options'));

		await waitFor(() => expect(screen.getByTestId('open-account-deletion')).toBeEnabled());
		expect(screen.queryByTestId('account-deletion-options-error')).not.toBeInTheDocument();
		expect(mockFetchDeletionOptions).toHaveBeenCalledTimes(2);
	});

	it('retries local cleanup without scheduling deletion a second time', async () => {
		mockDeactivateAccountLocally
			.mockRejectedValueOnce(
				new Error(
					'Lokale Kontodaten konnten nicht vollständig gelöscht werden. Bitte versuche es erneut.'
				)
			)
			.mockResolvedValueOnce(undefined);
		render(ProfilePage);
		const user = userEvent.setup();

		await user.click(await screen.findByTestId('open-account-deletion'));
		await user.type(screen.getByTestId('account-deletion-confirmation'), 'KONTO LÖSCHEN');
		await user.click(screen.getByTestId('confirm-account-deletion'));

		expect(await screen.findByRole('alert')).toHaveTextContent(/Lokale Kontodaten/);
		expect(screen.getByTestId('confirm-account-deletion')).toHaveTextContent(
			'Lokale Daten erneut löschen'
		);
		await user.click(screen.getByTestId('confirm-account-deletion'));

		await waitFor(() => expect(mockGoto).toHaveBeenCalledWith('/account/deletion'));
		expect(mockRequestAccountDeletion).toHaveBeenCalledOnce();
		expect(mockDeactivateAccountLocally).toHaveBeenCalledTimes(2);
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

	it('accepts astral characters up to the codepoint limits without native UTF-16 maxima', async () => {
		const displayName = '😀'.repeat(100);
		const locationValue = '📚'.repeat(255);
		mockUpdateProfile.mockResolvedValue({
			...profile,
			display_name: displayName,
			location: locationValue
		});
		render(ProfilePage);
		const user = userEvent.setup();
		const name = await screen.findByTestId('profile-display-name-input');
		const location = screen.getByTestId('profile-location-input');

		expect(name).not.toHaveAttribute('maxlength');
		expect(location).not.toHaveAttribute('maxlength');
		await fireEvent.input(name, { target: { value: displayName } });
		await fireEvent.input(location, { target: { value: locationValue } });
		await user.click(screen.getByTestId('save-profile'));

		await waitFor(() =>
			expect(mockUpdateProfile).toHaveBeenCalledWith({
				display_name: displayName,
				location: locationValue
			})
		);
		expect(screen.queryByText(/muss 2 bis 100 Zeichen/)).not.toBeInTheDocument();
	});

	it('clears a previous success message before client-side profile validation', async () => {
		render(ProfilePage);
		const user = userEvent.setup();
		const name = await screen.findByTestId('profile-display-name-input');

		await user.click(screen.getByTestId('save-profile'));
		expect(await screen.findByTestId('profile-success')).toHaveTextContent(
			'Profildaten gespeichert.'
		);
		await user.clear(name);
		await user.type(name, 'X');
		await user.click(screen.getByTestId('save-profile'));

		expect(await screen.findByText(/muss 2 bis 100 Zeichen/)).toBeVisible();
		expect(screen.queryByTestId('profile-success')).not.toBeInTheDocument();
		expect(mockUpdateProfile).toHaveBeenCalledOnce();
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

	it('clears a previous avatar success before rejecting a new file locally', async () => {
		render(ProfilePage);
		const user = userEvent.setup({ applyAccept: false });
		const input = await screen.findByTestId('profile-avatar-input');

		await user.upload(input, new File(['avatar'], 'avatar.png', { type: 'image/png' }));
		expect(await screen.findByTestId('profile-success')).toHaveTextContent('Avatar gespeichert.');
		await user.upload(input, new File(['text'], 'avatar.txt', { type: 'text/plain' }));

		expect(await screen.findByTestId('profile-error')).toHaveTextContent('JPEG-, PNG- oder WebP');
		expect(screen.queryByTestId('profile-success')).not.toBeInTheDocument();
		expect(mockUploadAvatar).toHaveBeenCalledOnce();
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
