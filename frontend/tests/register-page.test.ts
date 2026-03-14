import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import { userEvent } from '@testing-library/user-event';
import RegisterPage from '../src/routes/register/+page.svelte';

// Mock the API module
vi.mock('$lib/api/auth', () => ({
	register: vi.fn()
}));

// Mock SvelteKit modules
vi.mock('$app/navigation', () => ({
	goto: vi.fn()
}));

describe('Register Page', () => {
	beforeEach(() => {
		vi.clearAllMocks();
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
			privacy_consent: true
		});
	});

	it('shows error message on failed registration', async () => {
		const { register } = await import('$lib/api/auth');
		const mockRegister = vi.mocked(register);
		mockRegister.mockRejectedValue(new Error('An account with this email already exists'));

		render(RegisterPage);
		const user = userEvent.setup();

		await user.type(screen.getByLabelText(/anzeigename/i), 'Max');
		await user.type(screen.getByLabelText(/e-mail/i), 'existing@example.com');
		await user.type(screen.getByLabelText(/^passwort$/i), 'Kj$9mP!xL2@q');
		await user.type(screen.getByLabelText(/passwort bestätigen/i), 'Kj$9mP!xL2@q');
		await user.click(screen.getByLabelText(/datenschutzerklärung/i));
		await user.click(screen.getByRole('button', { name: /registrieren/i }));

		const errorAlert = await screen.findByRole('alert');
		expect(errorAlert).toHaveTextContent(/already exists/i);
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
