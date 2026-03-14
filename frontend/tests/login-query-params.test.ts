import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import LoginPage from '../src/routes/login/+page.svelte';

vi.mock('$lib/api/auth', () => ({
	login: vi.fn(),
	resendVerification: vi.fn()
}));

vi.mock('$app/navigation', () => ({
	goto: vi.fn()
}));

vi.mock('$app/paths', () => ({
	resolve: (path: string) => path
}));

vi.mock('$lib/stores/auth.svelte', () => ({
	initAuth: vi.fn().mockResolvedValue(undefined)
}));

// Use vi.hoisted so the variable exists before vi.mock is hoisted
const pageMock = vi.hoisted(() => ({
	url: new URL('http://localhost/login')
}));

vi.mock('$app/state', () => ({
	page: pageMock
}));

describe('Login Page - Query Parameters', () => {
	beforeEach(() => {
		vi.clearAllMocks();
		// Reset to default URL
		pageMock.url = new URL('http://localhost/login');
	});

	it('shows success message after registration', () => {
		pageMock.url = new URL('http://localhost/login?registered=true');

		render(LoginPage);

		expect(screen.getByTestId('login-success')).toHaveTextContent(/prüfe deine E-Mails/i);
	});

	it('shows success message after email verification', () => {
		pageMock.url = new URL('http://localhost/login?verified=true');

		render(LoginPage);

		expect(screen.getByTestId('login-success')).toHaveTextContent(/erfolgreich bestätigt/i);
	});

	it('shows error message for invalid verification link', () => {
		pageMock.url = new URL('http://localhost/login?verify_error=invalid');

		render(LoginPage);

		expect(screen.getByTestId('verify-error')).toHaveTextContent(/ungültig oder abgelaufen/i);
	});
});
