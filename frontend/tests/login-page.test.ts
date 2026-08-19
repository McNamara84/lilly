import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import { userEvent } from '@testing-library/user-event';
import LoginPage from '../src/routes/login/+page.svelte';

const pageState = vi.hoisted(() => ({
	url: new URL('http://localhost/login')
}));

// Mock the API module
vi.mock('$lib/api/auth', () => ({
	login: vi.fn(),
	resendVerification: vi.fn(),
	fetchAuthOptions: vi.fn(),
	startOAuth: vi.fn()
}));

// Mock SvelteKit modules
vi.mock('$app/navigation', () => ({
	goto: vi.fn()
}));

// Mock $app/state
vi.mock('$app/state', () => ({
	page: pageState
}));

// Mock auth store
vi.mock('$lib/stores/auth.svelte', () => ({
	initAuth: vi.fn().mockResolvedValue(undefined),
	deactivateAccountLocally: vi.fn().mockResolvedValue(undefined)
}));

describe('Login Page', () => {
	beforeEach(async () => {
		vi.clearAllMocks();
		pageState.url = new URL('http://localhost/login');
		window.history.replaceState({}, '', '/login');
		const { fetchAuthOptions } = await import('$lib/api/auth');
		vi.mocked(fetchAuthOptions).mockResolvedValue({
			privacy_policy: { version: 'test-v1', url: '/privacy' },
			oauth: { google: false, github: false }
		});
	});

	it('renders the login form with all elements', () => {
		render(LoginPage);

		expect(screen.getByRole('heading', { name: /LILLY/i })).toBeInTheDocument();
		expect(screen.getByLabelText(/e-mail/i)).toBeInTheDocument();
		expect(screen.getByLabelText(/passwort/i)).toBeInTheDocument();
		expect(screen.getByRole('button', { name: /anmelden/i })).toBeInTheDocument();
	});

	it('renders OAuth buttons as disabled', () => {
		render(LoginPage);

		const googleBtn = screen.getByRole('button', { name: /google/i });
		const githubBtn = screen.getByRole('button', { name: /github/i });

		expect(googleBtn).toBeDisabled();
		expect(githubBtn).toBeDisabled();
	});

	it('enables configured providers and starts OAuth login', async () => {
		const { fetchAuthOptions, startOAuth } = await import('$lib/api/auth');
		vi.mocked(fetchAuthOptions).mockResolvedValue({
			privacy_policy: { version: 'test-v1', url: '/privacy' },
			oauth: { google: true, github: false }
		});
		vi.mocked(startOAuth).mockReturnValue(new Promise(() => {}));
		render(LoginPage);
		const google = screen.getByTestId('oauth-google');
		await vi.waitFor(() => expect(google).toBeEnabled());

		await userEvent.setup().click(google);

		expect(startOAuth).toHaveBeenCalledWith('google', 'login');
		expect(screen.getByText('Weiterleitung …')).toBeInTheDocument();
		expect(screen.getByTestId('oauth-github')).toBeDisabled();
	});

	it('starts a configured GitHub login', async () => {
		const { fetchAuthOptions, startOAuth } = await import('$lib/api/auth');
		vi.mocked(fetchAuthOptions).mockResolvedValue({
			privacy_policy: { version: 'test-v1', url: '/privacy' },
			oauth: { google: false, github: true }
		});
		vi.mocked(startOAuth).mockReturnValue(new Promise(() => {}));
		render(LoginPage);
		const github = screen.getByTestId('oauth-github');
		await vi.waitFor(() => expect(github).toBeEnabled());

		await userEvent.setup().click(github);

		expect(startOAuth).toHaveBeenCalledWith('github', 'login');
		expect(screen.getByText('Weiterleitung …')).toBeInTheDocument();
	});

	it('reports OAuth startup errors and resets the loading state', async () => {
		const { fetchAuthOptions, startOAuth } = await import('$lib/api/auth');
		vi.mocked(fetchAuthOptions).mockResolvedValue({
			privacy_policy: { version: 'test-v1', url: '/privacy' },
			oauth: { google: true, github: false }
		});
		vi.mocked(startOAuth).mockRejectedValue(new Error('OAuth-Start fehlgeschlagen'));
		render(LoginPage);
		const google = screen.getByTestId('oauth-google');
		await vi.waitFor(() => expect(google).toBeEnabled());

		await userEvent.setup().click(google);

		expect(await screen.findByRole('alert')).toHaveTextContent('OAuth-Start fehlgeschlagen');
		expect(google).toBeEnabled();
	});

	it('navigates to the provider authorization URL after a successful OAuth start', async () => {
		const { fetchAuthOptions, startOAuth } = await import('$lib/api/auth');
		vi.mocked(fetchAuthOptions).mockResolvedValue({
			privacy_policy: { version: 'test-v1', url: '/privacy' },
			oauth: { google: true, github: false }
		});
		vi.mocked(startOAuth).mockResolvedValue('#google-authorization');
		render(LoginPage);
		const google = screen.getByTestId('oauth-google');
		await vi.waitFor(() => expect(google).toBeEnabled());

		await userEvent.setup().click(google);

		await vi.waitFor(() => expect(window.location.hash).toBe('#google-authorization'));
	});

	it('uses a safe fallback for untyped OAuth startup errors', async () => {
		const { fetchAuthOptions, startOAuth } = await import('$lib/api/auth');
		vi.mocked(fetchAuthOptions).mockResolvedValue({
			privacy_policy: { version: 'test-v1', url: '/privacy' },
			oauth: { google: true, github: false }
		});
		vi.mocked(startOAuth).mockRejectedValue('untyped failure');
		render(LoginPage);
		const google = screen.getByTestId('oauth-google');
		await vi.waitFor(() => expect(google).toBeEnabled());

		await userEvent.setup().click(google);

		expect(await screen.findByRole('alert')).toHaveTextContent(
			'OAuth konnte nicht gestartet werden.'
		);
	});

	it('explains when OAuth registration is required', () => {
		pageState.url = new URL('http://localhost/login?oauth_error=OAUTH_REGISTRATION_REQUIRED');
		render(LoginPage);

		expect(screen.getByTestId('oauth-error')).toHaveTextContent(/noch kein LILLY-Konto/i);
		expect(screen.getByRole('link', { name: /jetzt registrieren/i })).toHaveAttribute(
			'href',
			'/register'
		);
	});

	it.each([
		['OAUTH_PROVIDER_DENIED', 'Anmeldung beim Anbieter wurde abgebrochen'],
		['OAUTH_VERIFIED_EMAIL_REQUIRED', 'keine bestätigte primäre E-Mail-Adresse'],
		['OAUTH_STATE_INVALID', 'Anmeldevorgang ist abgelaufen oder ungültig'],
		['OAUTH_PROVIDER_DISABLED', 'Anmeldeanbieter ist derzeit nicht verfügbar'],
		['OAUTH_PROVIDER_ERROR', 'Anmeldeanbieter konnte nicht erreicht werden']
	])('shows the mapped OAuth error for %s', (code, message) => {
		pageState.url = new URL(`http://localhost/login?oauth_error=${code}`);

		render(LoginPage);

		expect(screen.getByTestId('oauth-error')).toHaveTextContent(message);
	});

	it('shows tagline text', () => {
		render(LoginPage);
		expect(
			screen.getByText(/Listing Inventory for Lovely Little Yellowbacks/i)
		).toBeInTheDocument();
	});

	it('shows registration link', () => {
		render(LoginPage);
		const link = screen.getByRole('link', { name: /registrieren/i });
		expect(link).toBeInTheDocument();
		expect(link).toHaveAttribute('href', '/register');
	});

	it('links to password recovery', () => {
		render(LoginPage);
		expect(screen.getByRole('link', { name: /passwort vergessen/i })).toHaveAttribute(
			'href',
			'/forgot-password'
		);
	});

	it('shows reset confirmation from the redirect', () => {
		pageState.url = new URL('http://localhost/login?reset=true');
		render(LoginPage);
		expect(screen.getByRole('status')).toHaveTextContent(/Passwort erfolgreich geändert/i);
	});

	it('shows email validation error on blur with empty field', async () => {
		render(LoginPage);
		const user = userEvent.setup();

		const emailInput = screen.getByLabelText(/e-mail/i);
		await user.click(emailInput);
		await user.tab();

		expect(screen.getByText(/E-Mail-Adresse ist erforderlich/i)).toBeInTheDocument();
	});

	it('shows email validation error for invalid format', async () => {
		render(LoginPage);
		const user = userEvent.setup();

		const emailInput = screen.getByLabelText(/e-mail/i);
		await user.type(emailInput, 'invalid-email');
		await user.tab();

		expect(screen.getByText(/gültige E-Mail-Adresse/i)).toBeInTheDocument();
	});

	it('shows password validation error on blur with empty field', async () => {
		render(LoginPage);
		const user = userEvent.setup();

		const passwordInput = screen.getByLabelText(/passwort/i);
		await user.click(passwordInput);
		await user.tab();

		expect(screen.getByText(/Passwort ist erforderlich/i)).toBeInTheDocument();
	});

	it('calls login API on valid form submission', async () => {
		const { login } = await import('$lib/api/auth');
		const mockLogin = vi.mocked(login);
		mockLogin.mockResolvedValue({ message: 'Login successful' });

		render(LoginPage);
		const user = userEvent.setup();

		await user.type(screen.getByLabelText(/e-mail/i), 'demo@lilly.app');
		await user.type(screen.getByLabelText(/passwort/i), 'demo1234');
		await user.click(screen.getByRole('button', { name: /anmelden/i }));

		expect(mockLogin).toHaveBeenCalledWith({
			email: 'demo@lilly.app',
			password: 'demo1234'
		});
	});

	it('returns to account linking after a successful login', async () => {
		pageState.url = new URL('http://localhost/login?return_to=%2Foauth%2Flink');
		const { login } = await import('$lib/api/auth');
		const { goto } = await import('$app/navigation');
		vi.mocked(login).mockResolvedValue({ message: 'Login successful' });
		render(LoginPage);
		const user = userEvent.setup();

		await user.type(screen.getByLabelText(/e-mail/i), 'demo@lilly.app');
		await user.type(screen.getByLabelText(/passwort/i), 'demo1234');
		await user.click(screen.getByRole('button', { name: /anmelden/i }));

		await vi.waitFor(() => expect(goto).toHaveBeenCalledWith('/oauth/link'));
	});

	it('purges local account data and opens recovery for a pending deletion', async () => {
		const { login } = await import('$lib/api/auth');
		const { goto } = await import('$app/navigation');
		const { deactivateAccountLocally } = await import('$lib/stores/auth.svelte');
		vi.mocked(login).mockResolvedValue({
			message: 'Account deletion is pending',
			account_state: 'pending_deletion',
			scheduled_for: '2026-08-24T08:00:00Z'
		});
		render(LoginPage);
		const user = userEvent.setup();

		await user.type(screen.getByLabelText(/e-mail/i), 'demo@lilly.app');
		await user.type(screen.getByLabelText(/passwort/i), 'demo1234');
		await user.click(screen.getByRole('button', { name: /anmelden/i }));

		await vi.waitFor(() => expect(deactivateAccountLocally).toHaveBeenCalledOnce());
		expect(goto).toHaveBeenCalledWith('/account/deletion');
	});

	it('shows error message on failed login', async () => {
		const { login } = await import('$lib/api/auth');
		const mockLogin = vi.mocked(login);
		mockLogin.mockRejectedValue(new Error('Invalid email or password'));

		render(LoginPage);
		const user = userEvent.setup();

		await user.type(screen.getByLabelText(/e-mail/i), 'wrong@email.com');
		await user.type(screen.getByLabelText(/passwort/i), 'wrongpassword');
		await user.click(screen.getByRole('button', { name: /anmelden/i }));

		const errorAlert = await screen.findByRole('alert');
		expect(errorAlert).toHaveTextContent(/Invalid email or password/i);
	});

	it('shows resend verification button when email is not verified', async () => {
		const { login } = await import('$lib/api/auth');
		const mockLogin = vi.mocked(login);
		const error = new Error('Email not verified') as Error & { code?: string };
		error.code = 'EMAIL_NOT_VERIFIED';
		mockLogin.mockRejectedValue(error);

		render(LoginPage);
		const user = userEvent.setup();

		await user.type(screen.getByLabelText(/e-mail/i), 'unverified@test.com');
		await user.type(screen.getByLabelText(/passwort/i), 'password123');
		await user.click(screen.getByRole('button', { name: /anmelden/i }));

		const resendButton = await screen.findByTestId('resend-verification-button');
		expect(resendButton).toBeInTheDocument();
	});

	it('sends resend verification email on button click', async () => {
		const { login, resendVerification } = await import('$lib/api/auth');
		const mockLogin = vi.mocked(login);
		const mockResend = vi.mocked(resendVerification);

		const error = new Error('Email not verified') as Error & { code?: string };
		error.code = 'EMAIL_NOT_VERIFIED';
		mockLogin.mockRejectedValue(error);
		mockResend.mockResolvedValue(undefined);

		render(LoginPage);
		const user = userEvent.setup();

		await user.type(screen.getByLabelText(/e-mail/i), 'unverified@test.com');
		await user.type(screen.getByLabelText(/passwort/i), 'password123');
		await user.click(screen.getByRole('button', { name: /anmelden/i }));

		const resendButton = await screen.findByTestId('resend-verification-button');
		await user.click(resendButton);

		expect(mockResend).toHaveBeenCalledWith('unverified@test.com');
		expect(await screen.findByText(/Bestätigungsmail wurde erneut gesendet/i)).toBeInTheDocument();
	});

	it('keeps the resend response indistinguishable when delivery fails', async () => {
		const { login, resendVerification } = await import('$lib/api/auth');
		const error = new Error('Email not verified') as Error & { code?: string };
		error.code = 'EMAIL_NOT_VERIFIED';
		vi.mocked(login).mockRejectedValue(error);
		vi.mocked(resendVerification).mockRejectedValue(new Error('Delivery failed'));
		render(LoginPage);
		const user = userEvent.setup();

		await user.type(screen.getByLabelText(/e-mail/i), 'unverified@test.com');
		await user.type(screen.getByLabelText(/passwort/i), 'password123');
		await user.click(screen.getByRole('button', { name: /anmelden/i }));
		await user.click(await screen.findByTestId('resend-verification-button'));

		expect(await screen.findByText(/Bestätigungsmail wurde erneut gesendet/i)).toBeInTheDocument();
	});

	it('shows generic error message on login failure without code', async () => {
		const { login } = await import('$lib/api/auth');
		const mockLogin = vi.mocked(login);
		mockLogin.mockRejectedValue(new Error(''));

		render(LoginPage);
		const user = userEvent.setup();

		await user.type(screen.getByLabelText(/e-mail/i), 'test@test.com');
		await user.type(screen.getByLabelText(/passwort/i), 'password123');
		await user.click(screen.getByRole('button', { name: /anmelden/i }));

		const errorAlert = await screen.findByRole('alert');
		expect(errorAlert).toHaveTextContent(/unerwarteter Fehler/i);
	});

	it('does not submit form when fields are invalid', async () => {
		const { login } = await import('$lib/api/auth');
		const mockLogin = vi.mocked(login);

		render(LoginPage);
		const user = userEvent.setup();

		// Click submit without filling in fields
		await user.click(screen.getByRole('button', { name: /anmelden/i }));

		expect(mockLogin).not.toHaveBeenCalled();
	});

	it('does not call resend when email is empty', async () => {
		const { login, resendVerification } = await import('$lib/api/auth');
		const mockLogin = vi.mocked(login);
		const mockResend = vi.mocked(resendVerification);

		const error = new Error('Email not verified') as Error & { code?: string };
		error.code = 'EMAIL_NOT_VERIFIED';
		mockLogin.mockRejectedValue(error);

		render(LoginPage);
		const user = userEvent.setup();

		// Type email, then clear it, then submit
		const emailInput = screen.getByLabelText(/e-mail/i);
		await user.type(emailInput, 'test@test.com');
		await user.type(screen.getByLabelText(/passwort/i), 'password');
		await user.click(screen.getByRole('button', { name: /anmelden/i }));

		// Now clear the email field before clicking resend
		await user.clear(emailInput);

		const resendButton = await screen.findByTestId('resend-verification-button');
		await user.click(resendButton);

		// resendVerification should not be called with empty email
		expect(mockResend).not.toHaveBeenCalled();
	});
});
