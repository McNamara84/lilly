import { beforeEach, describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import { userEvent } from '@testing-library/user-event';
import ForgotPasswordPage from '../src/routes/forgot-password/+page.svelte';

vi.mock('$lib/api/auth', () => ({
	requestPasswordReset: vi.fn()
}));

describe('Forgot Password Page', () => {
	beforeEach(() => {
		vi.clearAllMocks();
	});

	it('renders an accessible email form and login link', () => {
		render(ForgotPasswordPage);

		expect(screen.getByRole('heading', { name: /Passwort vergessen/i })).toBeInTheDocument();
		expect(screen.getByLabelText(/E-Mail/i)).toHaveAttribute('autocomplete', 'email');
		expect(screen.getByRole('button', { name: /Reset-Link anfordern/i })).toBeInTheDocument();
		expect(screen.getByRole('link', { name: /Zurück zur Anmeldung/i })).toHaveAttribute(
			'href',
			'/login'
		);
	});

	it('validates empty and malformed email addresses without calling the API', async () => {
		const { requestPasswordReset } = await import('$lib/api/auth');
		render(ForgotPasswordPage);
		const user = userEvent.setup();

		await user.click(screen.getByRole('button', { name: /Reset-Link anfordern/i }));
		expect(screen.getByText(/E-Mail-Adresse ist erforderlich/i)).toBeInTheDocument();
		await user.type(screen.getByLabelText(/E-Mail/i), 'invalid');
		await user.click(screen.getByRole('button', { name: /Reset-Link anfordern/i }));
		expect(screen.getByText(/gültige E-Mail-Adresse/i)).toBeInTheDocument();
		expect(requestPasswordReset).not.toHaveBeenCalled();
	});

	it('always shows the generic confirmation after success', async () => {
		const { requestPasswordReset } = await import('$lib/api/auth');
		vi.mocked(requestPasswordReset).mockResolvedValue({ message: 'generic' });
		render(ForgotPasswordPage);
		const user = userEvent.setup();

		await user.type(screen.getByLabelText(/E-Mail/i), 'unknown@example.com');
		await user.click(screen.getByRole('button', { name: /Reset-Link anfordern/i }));

		expect(requestPasswordReset).toHaveBeenCalledWith('unknown@example.com');
		expect(await screen.findByRole('status')).toHaveTextContent(/Falls für diese Adresse/i);
		expect(screen.queryByText('unknown@example.com')).not.toBeInTheDocument();
	});

	it('shows retry timing for rate-limited requests', async () => {
		const { requestPasswordReset } = await import('$lib/api/auth');
		const error = Object.assign(new Error('Too many requests'), { retry_after_seconds: 75 });
		vi.mocked(requestPasswordReset).mockRejectedValue(error);
		render(ForgotPasswordPage);
		const user = userEvent.setup();

		await user.type(screen.getByLabelText(/E-Mail/i), 'collector@example.com');
		await user.click(screen.getByRole('button', { name: /Reset-Link anfordern/i }));

		const alert = await screen.findByRole('alert');
		expect(alert).toHaveTextContent(/75 Sekunden/i);
		expect(alert).toHaveAttribute('data-retry-after', '75');
	});

	it('shows a safe fallback for untyped failures', async () => {
		const { requestPasswordReset } = await import('$lib/api/auth');
		vi.mocked(requestPasswordReset).mockRejectedValue(new Error('Dienst nicht verfügbar'));
		render(ForgotPasswordPage);
		const user = userEvent.setup();

		await user.type(screen.getByLabelText(/E-Mail/i), 'collector@example.com');
		await user.click(screen.getByRole('button', { name: /Reset-Link anfordern/i }));

		expect(await screen.findByRole('alert')).toHaveTextContent('Dienst nicht verfügbar');
	});
});
