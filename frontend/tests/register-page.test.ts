import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import { userEvent } from '@testing-library/user-event';
import RegisterPage from '../src/routes/register/+page.svelte';

const pageState = vi.hoisted(() => ({
	url: new URL('http://localhost/register')
}));

// Mock the API module
vi.mock('$lib/api/auth', () => ({
	register: vi.fn(),
	fetchAuthOptions: vi.fn(),
	startOAuth: vi.fn()
}));

// Mock SvelteKit modules
vi.mock('$app/navigation', () => ({
	goto: vi.fn()
}));

vi.mock('$app/state', () => ({ page: pageState }));

describe('Register Page', () => {
	beforeEach(async () => {
		vi.clearAllMocks();
		pageState.url = new URL('http://localhost/register');
		window.history.replaceState({}, '', '/register');
		const { fetchAuthOptions } = await import('$lib/api/auth');
		vi.mocked(fetchAuthOptions).mockResolvedValue({
			privacy_policy: { version: 'test-v1', url: '/privacy' },
			oauth: { google: true, github: true }
		});
	});

	it('renders the registration form with all elements', () => {
		render(RegisterPage);

		expect(screen.getByRole('heading', { name: /LILLY/i })).toBeInTheDocument();
		expect(screen.getByLabelText(/anzeigename/i)).toBeInTheDocument();
		expect(screen.getByLabelText(/e-mail/i)).toBeInTheDocument();
		expect(screen.getByLabelText(/^passwort$/i)).toBeInTheDocument();
		expect(screen.getByLabelText(/passwort bestätigen/i)).toBeInTheDocument();
		expect(screen.getByLabelText(/datenschutzerklärung/i)).toBeInTheDocument();
		expect(screen.getByRole('button', { name: /registrieren/i })).toBeInTheDocument();
	});

	it('renders OAuth buttons as disabled', () => {
		render(RegisterPage);

		const googleBtn = screen.getByRole('button', { name: /google/i });
		const githubBtn = screen.getByRole('button', { name: /github/i });

		expect(googleBtn).toBeDisabled();
		expect(githubBtn).toBeDisabled();
	});

	it('requires consent and sends its version when starting OAuth registration', async () => {
		const { startOAuth } = await import('$lib/api/auth');
		vi.mocked(startOAuth).mockReturnValue(new Promise(() => {}));
		render(RegisterPage);
		const google = screen.getByTestId('oauth-google');
		await vi.waitFor(() =>
			expect(screen.getByRole('link', { name: /Version test-v1/i })).toBeInTheDocument()
		);
		expect(google).toBeDisabled();

		await userEvent.setup().click(screen.getByLabelText(/datenschutzerklärung/i));
		expect(google).toBeEnabled();
		await userEvent.setup().click(google);

		expect(startOAuth).toHaveBeenCalledWith('google', 'register', {
			privacy_consent: true,
			privacy_policy_version: 'test-v1'
		});
		expect(screen.getByText('Weiterleitung …')).toBeInTheDocument();
	});

	it('starts a configured GitHub registration', async () => {
		const { startOAuth } = await import('$lib/api/auth');
		vi.mocked(startOAuth).mockReturnValue(new Promise(() => {}));
		render(RegisterPage);
		await screen.findByRole('link', { name: /Version test-v1/i });
		const user = userEvent.setup();
		await user.click(screen.getByLabelText(/datenschutzerklärung/i));

		await user.click(screen.getByTestId('oauth-github'));

		expect(startOAuth).toHaveBeenCalledWith('github', 'register', {
			privacy_consent: true,
			privacy_policy_version: 'test-v1'
		});
		expect(screen.getByText('Weiterleitung …')).toBeInTheDocument();
	});

	it('reloads policy options when OAuth reports a version conflict', async () => {
		const { fetchAuthOptions, startOAuth } = await import('$lib/api/auth');
		const staleError = new Error('Policy changed') as Error & { code?: string };
		staleError.code = 'PRIVACY_POLICY_CHANGED';
		vi.mocked(startOAuth).mockRejectedValue(staleError);
		render(RegisterPage);
		await screen.findByRole('link', { name: /Version test-v1/i });
		vi.mocked(fetchAuthOptions).mockResolvedValue({
			privacy_policy: { version: 'test-v2', url: '/privacy' },
			oauth: { google: true, github: true }
		});
		const user = userEvent.setup();
		const consent = screen.getByLabelText(/datenschutzerklärung/i);
		await user.click(consent);

		await user.click(screen.getByTestId('oauth-google'));

		await vi.waitFor(() => expect(consent).not.toBeChecked());
		expect(screen.getByRole('link', { name: /Version test-v2/i })).toBeInTheDocument();
		expect(screen.getByRole('alert')).toHaveTextContent('Policy changed');
	});

	it('navigates to the provider authorization URL after a successful OAuth start', async () => {
		const { startOAuth } = await import('$lib/api/auth');
		vi.mocked(startOAuth).mockResolvedValue('#github-authorization');
		render(RegisterPage);
		await screen.findByRole('link', { name: /Version test-v1/i });
		const user = userEvent.setup();
		await user.click(screen.getByLabelText(/datenschutzerklärung/i));

		await user.click(screen.getByTestId('oauth-github'));

		await vi.waitFor(() => expect(window.location.hash).toBe('#github-authorization'));
	});

	it('uses a safe fallback for untyped OAuth startup errors', async () => {
		const { startOAuth } = await import('$lib/api/auth');
		vi.mocked(startOAuth).mockRejectedValue('untyped failure');
		render(RegisterPage);
		await screen.findByRole('link', { name: /Version test-v1/i });
		const user = userEvent.setup();
		await user.click(screen.getByLabelText(/datenschutzerklärung/i));

		await user.click(screen.getByTestId('oauth-google'));

		expect(await screen.findByRole('alert')).toHaveTextContent(
			'OAuth konnte nicht gestartet werden.'
		);
		expect(screen.getByTestId('oauth-google')).toBeEnabled();
	});

	it.each([
		['PRIVACY_CONSENT_REQUIRED', 'ausdrückliche Datenschutz-Einwilligung'],
		['OAUTH_PROVIDER_DENIED', 'Registrierung beim Anbieter wurde abgebrochen'],
		['OAUTH_VERIFIED_EMAIL_REQUIRED', 'keine bestätigte primäre E-Mail-Adresse'],
		['OAUTH_STATE_INVALID', 'Registrierungsvorgang ist abgelaufen oder ungültig'],
		['OAUTH_PROVIDER_DISABLED', 'Registrierungsanbieter ist derzeit nicht verfügbar'],
		['OAUTH_PROVIDER_ERROR', 'Registrierungsanbieter konnte nicht erreicht werden']
	])('shows the mapped OAuth registration error for %s', (code, message) => {
		pageState.url = new URL(`http://localhost/register?oauth_error=${code}`);

		render(RegisterPage);

		expect(screen.getByRole('alert')).toHaveTextContent(message);
	});

	it('reports typed and untyped failures while loading registration options', async () => {
		const { fetchAuthOptions } = await import('$lib/api/auth');
		vi.mocked(fetchAuthOptions).mockRejectedValueOnce(new Error('Optionen nicht verfügbar'));
		const typedFailure = render(RegisterPage);
		expect(await screen.findByRole('alert')).toHaveTextContent('Optionen nicht verfügbar');
		typedFailure.unmount();

		vi.mocked(fetchAuthOptions).mockRejectedValueOnce('untyped failure');
		render(RegisterPage);
		expect(await screen.findByRole('alert')).toHaveTextContent(
			'Registrierungsoptionen konnten nicht geladen werden.'
		);
	});

	it('shows a callback privacy-version error', () => {
		pageState.url = new URL('http://localhost/register?oauth_error=PRIVACY_POLICY_CHANGED');
		render(RegisterPage);

		expect(screen.getByRole('alert')).toHaveTextContent(/Datenschutzerklärung wurde geändert/i);
	});

	it('shows link to login page', () => {
		render(RegisterPage);
		const link = screen.getByRole('link', { name: /anmelden/i });
		expect(link).toBeInTheDocument();
		expect(link).toHaveAttribute('href', '/login');
	});

	it('shows link to privacy policy', () => {
		render(RegisterPage);
		const link = screen.getByRole('link', { name: /datenschutzerklärung/i });
		expect(link).toHaveAttribute('href', '/privacy');
	});

	it('shows display name validation error on blur with empty field', async () => {
		render(RegisterPage);
		const user = userEvent.setup();

		const input = screen.getByLabelText(/anzeigename/i);
		await user.click(input);
		await user.tab();

		expect(screen.getByText(/Anzeigename ist erforderlich/i)).toBeInTheDocument();
	});

	it('shows email validation error on blur with empty field', async () => {
		render(RegisterPage);
		const user = userEvent.setup();

		const input = screen.getByLabelText(/e-mail/i);
		await user.click(input);
		await user.tab();

		expect(screen.getByText(/E-Mail-Adresse ist erforderlich/i)).toBeInTheDocument();
	});

	it('shows client-side errors for malformed email and incomplete passwords', async () => {
		render(RegisterPage);
		const user = userEvent.setup();
		const email = screen.getByLabelText(/e-mail/i);
		await user.type(email, 'invalid-email');
		await user.tab();

		expect(screen.getByText(/gültige E-Mail-Adresse/i)).toBeInTheDocument();

		const password = screen.getByLabelText(/^passwort$/i);
		await user.type(password, 'short');
		await user.tab();
		expect(screen.getByText(/mindestens 8 Zeichen/i)).toBeInTheDocument();

		const confirmation = screen.getByLabelText(/passwort bestätigen/i);
		await user.click(confirmation);
		await user.tab();
		expect(screen.getByText(/Passwortbestätigung ist erforderlich/i)).toBeInTheDocument();
	});

	it('rejects a long but weak password on blur', async () => {
		render(RegisterPage);
		const user = userEvent.setup();
		const password = screen.getByLabelText(/^passwort$/i);

		await user.type(password, 'aaaaaaaaaaaa');
		await user.tab();

		expect(screen.getByText(/Passwort ist zu schwach/i)).toBeInTheDocument();
	});

	it('shows password strength indicator when typing', async () => {
		render(RegisterPage);
		const user = userEvent.setup();

		const input = screen.getByLabelText(/^passwort$/i);
		await user.type(input, 'testpassword');

		expect(screen.getByTestId('password-strength')).toBeInTheDocument();
	});

	it('shows password confirmation error on mismatch', async () => {
		render(RegisterPage);
		const user = userEvent.setup();

		await user.type(screen.getByLabelText(/^passwort$/i), 'Password123!');
		const confirmInput = screen.getByLabelText(/passwort bestätigen/i);
		await user.type(confirmInput, 'DifferentPassword');
		await user.tab();

		expect(screen.getByText(/Passwörter stimmen nicht überein/i)).toBeInTheDocument();
	});

	it('calls register API on valid form submission', async () => {
		const { register } = await import('$lib/api/auth');
		const mockRegister = vi.mocked(register);
		mockRegister.mockResolvedValue({ message: 'Registration successful.' });

		render(RegisterPage);
		const user = userEvent.setup();

		await user.type(screen.getByLabelText(/anzeigename/i), 'Max Mustermann');
		await user.type(screen.getByLabelText(/e-mail/i), 'max@example.com');
		await user.type(screen.getByLabelText(/^passwort$/i), 'Kj$9mP!xL2@q');
		await user.type(screen.getByLabelText(/passwort bestätigen/i), 'Kj$9mP!xL2@q');
		await user.click(screen.getByLabelText(/datenschutzerklärung/i));
		await user.click(screen.getByRole('button', { name: /registrieren/i }));

		expect(mockRegister).toHaveBeenCalledWith({
			display_name: 'Max Mustermann',
			email: 'max@example.com',
			password: 'Kj$9mP!xL2@q', // ggignore
			password_confirmation: 'Kj$9mP!xL2@q', // ggignore
			privacy_consent: true,
			privacy_policy_version: 'test-v1'
		});
	});

	it('shows error message on failed registration', async () => {
		const { register } = await import('$lib/api/auth');
		const mockRegister = vi.mocked(register);
		mockRegister.mockRejectedValue(
			new Error('Password is too weak. Please choose a stronger password.')
		);

		render(RegisterPage);
		const user = userEvent.setup();

		await user.type(screen.getByLabelText(/anzeigename/i), 'Max');
		await user.type(screen.getByLabelText(/e-mail/i), 'existing@example.com');
		await user.type(screen.getByLabelText(/^passwort$/i), 'Kj$9mP!xL2@q');
		await user.type(screen.getByLabelText(/passwort bestätigen/i), 'Kj$9mP!xL2@q');
		await user.click(screen.getByLabelText(/datenschutzerklärung/i));
		await user.click(screen.getByRole('button', { name: /registrieren/i }));

		const errorAlert = await screen.findByRole('alert');
		expect(errorAlert).toHaveTextContent(/too weak/i);
	});

	it('renders field-specific errors returned by the backend', async () => {
		const { register } = await import('$lib/api/auth');
		const validationError = new Error('Validation failed') as Error & {
			fields?: Record<string, string>;
		};
		validationError.fields = {
			email: 'Diese E-Mail-Adresse ist ungültig',
			privacy_consent: 'Die Datenschutz-Einwilligung ist erforderlich'
		};
		vi.mocked(register).mockRejectedValue(validationError);
		render(RegisterPage);
		const user = userEvent.setup();

		await user.type(screen.getByLabelText(/anzeigename/i), 'Max');
		await user.type(screen.getByLabelText(/e-mail/i), 'max@example.com');
		await user.type(screen.getByLabelText(/^passwort$/i), 'Kj$9mP!xL2@q');
		await user.type(screen.getByLabelText(/passwort bestätigen/i), 'Kj$9mP!xL2@q');
		await user.click(screen.getByLabelText(/datenschutzerklärung/i));
		await user.click(screen.getByRole('button', { name: /registrieren/i }));

		expect(await screen.findByText('Diese E-Mail-Adresse ist ungültig')).toBeInTheDocument();
		const consentError = screen.getByText('Die Datenschutz-Einwilligung ist erforderlich');
		expect(consentError).toHaveAttribute('id', 'privacy-consent-error');
		expect(screen.getByLabelText(/datenschutzerklärung/i)).toHaveAttribute('aria-invalid', 'true');
		expect(screen.getByLabelText(/datenschutzerklärung/i)).toHaveAttribute(
			'aria-describedby',
			'privacy-consent-error'
		);
		expect(screen.getByRole('alert')).toHaveTextContent(/markierten Felder/i);
	});

	it('uses a safe fallback for an untyped registration failure', async () => {
		const { register } = await import('$lib/api/auth');
		vi.mocked(register).mockRejectedValue('untyped failure');
		render(RegisterPage);
		const user = userEvent.setup();

		await user.type(screen.getByLabelText(/anzeigename/i), 'Max');
		await user.type(screen.getByLabelText(/e-mail/i), 'max@example.com');
		await user.type(screen.getByLabelText(/^passwort$/i), 'Kj$9mP!xL2@q');
		await user.type(screen.getByLabelText(/passwort bestätigen/i), 'Kj$9mP!xL2@q');
		await user.click(screen.getByLabelText(/datenschutzerklärung/i));
		await user.click(screen.getByRole('button', { name: /registrieren/i }));

		expect(await screen.findByRole('alert')).toHaveTextContent(
			'Ein unerwarteter Fehler ist aufgetreten'
		);
	});

	it('resets consent and reloads policy options after a version conflict', async () => {
		const { register, fetchAuthOptions } = await import('$lib/api/auth');
		const staleError = new Error('Policy changed') as Error & { code?: string };
		staleError.code = 'PRIVACY_POLICY_CHANGED';
		vi.mocked(register).mockRejectedValue(staleError);
		render(RegisterPage);
		await screen.findByRole('link', { name: /Version test-v1/i });
		vi.mocked(fetchAuthOptions).mockResolvedValue({
			privacy_policy: { version: 'test-v2', url: '/privacy' },
			oauth: { google: true, github: true }
		});
		const user = userEvent.setup();

		await user.type(screen.getByLabelText(/anzeigename/i), 'Max');
		await user.type(screen.getByLabelText(/e-mail/i), 'max@example.com');
		await user.type(screen.getByLabelText(/^passwort$/i), 'Kj$9mP!xL2@q');
		await user.type(screen.getByLabelText(/passwort bestätigen/i), 'Kj$9mP!xL2@q');
		const consent = screen.getByLabelText(/datenschutzerklärung/i);
		await user.click(consent);
		await user.click(screen.getByRole('button', { name: /registrieren/i }));

		await vi.waitFor(() => expect(consent).not.toBeChecked());
		expect(screen.getByRole('link', { name: /Version test-v2/i })).toBeInTheDocument();
	});

	it('redirects to login on successful registration', async () => {
		const { register } = await import('$lib/api/auth');
		const { goto } = await import('$app/navigation');
		const mockRegister = vi.mocked(register);
		const mockGoto = vi.mocked(goto);
		mockRegister.mockResolvedValue({ message: 'Registration successful.' });

		render(RegisterPage);
		const user = userEvent.setup();

		await user.type(screen.getByLabelText(/anzeigename/i), 'Max');
		await user.type(screen.getByLabelText(/e-mail/i), 'new@example.com');
		await user.type(screen.getByLabelText(/^passwort$/i), 'Kj$9mP!xL2@q');
		await user.type(screen.getByLabelText(/passwort bestätigen/i), 'Kj$9mP!xL2@q');
		await user.click(screen.getByLabelText(/datenschutzerklärung/i));
		await user.click(screen.getByRole('button', { name: /registrieren/i }));

		// Wait for the async operation to complete
		await vi.waitFor(() => {
			expect(mockGoto).toHaveBeenCalledWith('/login?registered=true');
		});
	});
});
