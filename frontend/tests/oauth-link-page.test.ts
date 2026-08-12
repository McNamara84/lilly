import { beforeEach, describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import { userEvent } from '@testing-library/user-event';
import OAuthLinkPage from '../src/routes/oauth/link/+page.svelte';

const authState = vi.hoisted(() => ({
	user: null as null | { display_name: string },
	isLoading: false,
	isAuthenticated: false
}));

vi.mock('$lib/api/auth', () => ({
	fetchPendingOAuthLink: vi.fn(),
	confirmOAuthLink: vi.fn(),
	cancelOAuthLink: vi.fn()
}));

vi.mock('$lib/stores/auth.svelte', () => ({
	getAuthState: () => authState
}));

vi.mock('$app/navigation', () => ({ goto: vi.fn() }));

describe('OAuth link page', () => {
	beforeEach(async () => {
		vi.clearAllMocks();
		authState.user = null;
		authState.isLoading = false;
		authState.isAuthenticated = false;
		const { fetchPendingOAuthLink } = await import('$lib/api/auth');
		vi.mocked(fetchPendingOAuthLink).mockResolvedValue({
			pending: true,
			provider: 'github',
			masked_email: 'c***@example.test',
			expires_at: '2026-08-12T10:00:00',
			confirmation_token: 'one-time-confirmation'
		});
	});

	it('shows only masked provider data and requires an existing login', async () => {
		render(OAuthLinkPage);

		expect(await screen.findByText('c***@example.test')).toBeInTheDocument();
		expect(screen.queryByText('collector@example.test')).not.toBeInTheDocument();
		expect(screen.getByTestId('oauth-link-login')).toHaveAttribute(
			'href',
			'/login?return_to=%2Foauth%2Flink'
		);
		expect(screen.queryByTestId('oauth-link-confirm')).not.toBeInTheDocument();
	});

	it('waits for authentication state before offering a link action', async () => {
		authState.isLoading = true;
		render(OAuthLinkPage);

		expect(await screen.findByText('Kontostatus wird geprüft …')).toBeInTheDocument();
		expect(screen.queryByTestId('oauth-link-login')).not.toBeInTheDocument();
		expect(screen.queryByTestId('oauth-link-confirm')).not.toBeInTheDocument();
	});

	it.each([
		['google', 'Google'],
		[undefined, 'OAuth']
	])('uses the expected provider label for %s', async (provider, label) => {
		const { fetchPendingOAuthLink } = await import('$lib/api/auth');
		vi.mocked(fetchPendingOAuthLink).mockResolvedValue({
			pending: true,
			provider: provider as 'google' | undefined,
			masked_email: 'c***@example.test',
			expires_at: '2026-08-12T10:00:00',
			confirmation_token: 'one-time-confirmation'
		});

		render(OAuthLinkPage);

		expect(await screen.findByText(new RegExp(`${label}-Adresse`))).toBeInTheDocument();
	});

	it('links only after explicit confirmation by an authenticated user', async () => {
		authState.user = { display_name: 'Collector' };
		authState.isAuthenticated = true;
		const { confirmOAuthLink } = await import('$lib/api/auth');
		const { goto } = await import('$app/navigation');
		vi.mocked(confirmOAuthLink).mockResolvedValue(undefined);
		render(OAuthLinkPage);

		await userEvent.setup().click(await screen.findByTestId('oauth-link-confirm'));

		expect(confirmOAuthLink).toHaveBeenCalledWith('one-time-confirmation');
		expect(goto).toHaveBeenCalledWith('/profile');
	});

	it('shows a confirmation failure without navigating', async () => {
		authState.user = { display_name: 'Wrong Collector' };
		authState.isAuthenticated = true;
		const { confirmOAuthLink } = await import('$lib/api/auth');
		const { goto } = await import('$app/navigation');
		vi.mocked(confirmOAuthLink).mockRejectedValue(new Error('Passendes Konto erforderlich'));
		render(OAuthLinkPage);

		await userEvent.setup().click(await screen.findByTestId('oauth-link-confirm'));

		expect(await screen.findByRole('alert')).toHaveTextContent('Passendes Konto erforderlich');
		expect(goto).not.toHaveBeenCalled();
	});

	it('uses a safe fallback for an untyped confirmation failure', async () => {
		authState.user = { display_name: 'Collector' };
		authState.isAuthenticated = true;
		const { confirmOAuthLink } = await import('$lib/api/auth');
		vi.mocked(confirmOAuthLink).mockRejectedValue('untyped failure');
		render(OAuthLinkPage);

		await userEvent.setup().click(await screen.findByTestId('oauth-link-confirm'));

		expect(await screen.findByRole('alert')).toHaveTextContent('Verknüpfung ist fehlgeschlagen.');
	});

	it('refuses confirmation when the pending response has no one-time token', async () => {
		authState.user = { display_name: 'Collector' };
		authState.isAuthenticated = true;
		const { fetchPendingOAuthLink, confirmOAuthLink } = await import('$lib/api/auth');
		vi.mocked(fetchPendingOAuthLink).mockResolvedValue({
			pending: true,
			provider: 'google',
			masked_email: 'c***@example.test'
		});
		render(OAuthLinkPage);

		await userEvent.setup().click(await screen.findByTestId('oauth-link-confirm'));

		expect(confirmOAuthLink).not.toHaveBeenCalled();
		expect(await screen.findByRole('alert')).toHaveTextContent(/abgelaufen oder nicht vorhanden/i);
	});

	it('allows an unauthenticated pending link to be cancelled', async () => {
		const { cancelOAuthLink } = await import('$lib/api/auth');
		const { goto } = await import('$app/navigation');
		vi.mocked(cancelOAuthLink).mockResolvedValue(undefined);
		render(OAuthLinkPage);

		await userEvent.setup().click(await screen.findByTestId('oauth-link-cancel'));

		expect(cancelOAuthLink).toHaveBeenCalledOnce();
		expect(goto).toHaveBeenCalledWith('/login');
	});

	it('returns an authenticated user to the profile after cancelling', async () => {
		authState.user = { display_name: 'Collector' };
		authState.isAuthenticated = true;
		const { cancelOAuthLink } = await import('$lib/api/auth');
		const { goto } = await import('$app/navigation');
		vi.mocked(cancelOAuthLink).mockResolvedValue(undefined);
		render(OAuthLinkPage);

		await userEvent.setup().click(await screen.findByTestId('oauth-link-cancel'));

		expect(goto).toHaveBeenCalledWith('/profile');
	});

	it.each([
		[new Error('Abbruch fehlgeschlagen'), 'Abbruch fehlgeschlagen'],
		['untyped failure', 'Verknüpfung konnte nicht abgebrochen werden.']
	])('reports a cancellation failure without navigating', async (failure, message) => {
		const { cancelOAuthLink } = await import('$lib/api/auth');
		const { goto } = await import('$app/navigation');
		vi.mocked(cancelOAuthLink).mockRejectedValue(failure);
		render(OAuthLinkPage);

		await userEvent.setup().click(await screen.findByTestId('oauth-link-cancel'));

		expect(await screen.findByRole('alert')).toHaveTextContent(message);
		expect(goto).not.toHaveBeenCalled();
	});

	it('reports an expired or missing pending link', async () => {
		const { fetchPendingOAuthLink } = await import('$lib/api/auth');
		vi.mocked(fetchPendingOAuthLink).mockResolvedValue({ pending: false });

		render(OAuthLinkPage);

		expect(await screen.findByRole('alert')).toHaveTextContent(/abgelaufen oder nicht vorhanden/i);
	});

	it('reports a pending-link loading failure', async () => {
		const { fetchPendingOAuthLink } = await import('$lib/api/auth');
		vi.mocked(fetchPendingOAuthLink).mockRejectedValue(new Error('Netzwerkfehler'));

		render(OAuthLinkPage);

		expect(await screen.findByRole('alert')).toHaveTextContent('Netzwerkfehler');
	});

	it('uses a safe fallback for an untyped pending-link failure', async () => {
		const { fetchPendingOAuthLink } = await import('$lib/api/auth');
		vi.mocked(fetchPendingOAuthLink).mockRejectedValue('untyped failure');

		render(OAuthLinkPage);

		expect(await screen.findByRole('alert')).toHaveTextContent(
			'Verknüpfung konnte nicht geladen werden.'
		);
	});
});
