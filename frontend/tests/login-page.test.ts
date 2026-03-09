import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import { userEvent } from '@testing-library/user-event';
import LoginPage from '../src/routes/login/+page.svelte';

// Mock the API module
vi.mock('../src/lib/api/auth', () => ({
	login: vi.fn()
}));

// Mock SvelteKit modules
vi.mock('$app/navigation', () => ({
	goto: vi.fn()
}));

describe('Login Page', () => {
	beforeEach(() => {
		vi.clearAllMocks();
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

	it('shows tagline text', () => {
		render(LoginPage);
		expect(
			screen.getByText(/Listing Inventory for Lovely Little Yellowbacks/i)
		).toBeInTheDocument();
	});

	it('shows registration and password reset links', () => {
		render(LoginPage);
		expect(screen.getByText(/Registrieren/i)).toBeInTheDocument();
		expect(screen.getByText(/Passwort vergessen/i)).toBeInTheDocument();
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
		const { login } = await import('../src/lib/api/auth');
		const mockLogin = vi.mocked(login);
		mockLogin.mockResolvedValue({
			access_token: 'test-token',
			token_type: 'Bearer',
			expires_in: 900
		});

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

	it('shows error message on failed login', async () => {
		const { login } = await import('../src/lib/api/auth');
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
});
