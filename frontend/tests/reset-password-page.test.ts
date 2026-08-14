import { beforeEach, describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import { userEvent } from '@testing-library/user-event';
import ResetPasswordPage from '../src/routes/reset-password/+page.svelte';

const pageState = vi.hoisted(() => ({
	url: new URL(`http://localhost/reset-password?token=${'a'.repeat(43)}`)
}));

vi.mock('$lib/api/auth', () => ({
	confirmPasswordReset: vi.fn()
}));

vi.mock('$app/navigation', () => ({
	goto: vi.fn(),
	replaceState: vi.fn()
}));

vi.mock('$app/state', () => ({
	page: pageState
}));

describe('Reset Password Page', () => {
	beforeEach(() => {
		vi.clearAllMocks();
		pageState.url = new URL(`http://localhost/reset-password?token=${'a'.repeat(43)}`);
		window.history.replaceState({}, '', pageState.url.pathname + pageState.url.search);
	});

	it('rejects a missing token without rendering the password form', () => {
		pageState.url = new URL('http://localhost/reset-password');
		render(ResetPasswordPage);

		expect(screen.getByRole('alert')).toHaveTextContent(/unvollständig oder ungültig/i);
		expect(screen.queryByTestId('new-password-input')).not.toBeInTheDocument();
		expect(screen.getByRole('link', { name: /Neuen Reset-Link/i })).toHaveAttribute(
			'href',
			'/forgot-password'
		);
	});

	it('shows password strength and validates confirmation', async () => {
		render(ResetPasswordPage);
		const user = userEvent.setup();
		const password = screen.getByLabelText('Neues Passwort');

		await user.type(password, 'password');
		await user.tab();
		expect(screen.getByTestId('password-strength')).toBeInTheDocument();
		expect(screen.getByText(/Passwort ist zu schwach/i)).toBeInTheDocument();
		await user.type(screen.getByLabelText(/Passwort bestätigen/i), 'different password');
		await user.tab();
		expect(screen.getByText(/Passwörter stimmen nicht überein/i)).toBeInTheDocument();
	});

	it('only references rendered password descriptions', async () => {
		render(ResetPasswordPage);
		const user = userEvent.setup();
		const password = screen.getByLabelText('Neues Passwort');

		expect(password).not.toHaveAttribute('aria-describedby');

		await user.type(password, 'password');
		expect(password).toHaveAttribute('aria-describedby', 'reset-password-strength');
		expect(document.getElementById('reset-password-strength')).toBeInTheDocument();

		await user.clear(password);
		await user.tab();
		expect(password).toHaveAttribute('aria-describedby', 'new-password-error');
		expect(document.getElementById('reset-password-strength')).not.toBeInTheDocument();
		expect(document.getElementById('new-password-error')).toBeInTheDocument();
	});

	it('submits a valid password and removes the token from browser history', async () => {
		const { confirmPasswordReset } = await import('$lib/api/auth');
		const { goto, replaceState } = await import('$app/navigation');
		vi.mocked(confirmPasswordReset).mockResolvedValue({ message: 'success' });
		render(ResetPasswordPage);
		const user = userEvent.setup();
		const strongPassword = 'correct horse battery staple 2049!';

		await user.type(screen.getByLabelText('Neues Passwort'), strongPassword);
		await user.type(screen.getByLabelText(/Passwort bestätigen/i), strongPassword);
		await user.click(screen.getByRole('button', { name: /Passwort ändern/i }));

		expect(confirmPasswordReset).toHaveBeenCalledWith({
			token: 'a'.repeat(43),
			password: strongPassword,
			password_confirmation: strongPassword
		});
		await vi.waitFor(() => expect(goto).toHaveBeenCalledWith('/login?reset=true'));
		expect(replaceState).toHaveBeenCalledWith('/reset-password', {});
	});

	it('maps invalid token, field and rate-limit errors', async () => {
		const { confirmPasswordReset } = await import('$lib/api/auth');
		const user = userEvent.setup();
		const strongPassword = 'correct horse battery staple 2049!';
		let view = render(ResetPasswordPage);
		await user.type(screen.getByLabelText('Neues Passwort'), strongPassword);
		await user.type(screen.getByLabelText(/Passwort bestätigen/i), strongPassword);

		vi.mocked(confirmPasswordReset).mockRejectedValueOnce(
			Object.assign(new Error('invalid'), { code: 'PASSWORD_RESET_TOKEN_INVALID' })
		);
		await user.click(screen.getByRole('button', { name: /Passwort ändern/i }));
		expect(await screen.findByRole('alert')).toHaveTextContent(/ungültig, abgelaufen/i);

		view.unmount();
		view = render(ResetPasswordPage);
		await user.type(screen.getByLabelText('Neues Passwort'), strongPassword);
		await user.type(screen.getByLabelText(/Passwort bestätigen/i), strongPassword);
		vi.mocked(confirmPasswordReset).mockRejectedValueOnce(
			Object.assign(new Error('fields'), { fields: { password: 'Server password error' } })
		);
		await user.click(screen.getByRole('button', { name: /Passwort ändern/i }));
		expect(await screen.findByText('Server password error')).toBeInTheDocument();

		view.unmount();
		view = render(ResetPasswordPage);
		await user.type(screen.getByLabelText('Neues Passwort'), strongPassword);
		await user.type(screen.getByLabelText(/Passwort bestätigen/i), strongPassword);
		vi.mocked(confirmPasswordReset).mockRejectedValueOnce(
			Object.assign(new Error('limited'), { retry_after_seconds: 33 })
		);
		await user.click(screen.getByRole('button', { name: /Passwort ändern/i }));
		expect(await screen.findByRole('alert')).toHaveTextContent(/33 Sekunden/i);
		view.unmount();
	});
});
