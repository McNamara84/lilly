import { beforeEach, describe, expect, it, vi } from 'vitest';
import { render, screen, waitFor } from '@testing-library/svelte';
import { createRawSnippet } from 'svelte';
import OfflineNotice from '../src/lib/components/offline/OfflineNotice.svelte';
import OnlineOnly from '../src/lib/components/offline/OnlineOnly.svelte';
import TradesLayout from '../src/routes/trades/+layout.svelte';
import MessagesLayout from '../src/routes/messages/+layout.svelte';
import LoginPage from '../src/routes/login/+page.svelte';
import RegisterPage from '../src/routes/register/+page.svelte';

const connection = vi.hoisted(() => ({ online: false }));

vi.mock('$lib/offline/status.svelte', () => ({
	getOfflineStatus: () => ({
		get online() {
			return connection.online;
		}
	})
}));

vi.mock('$app/paths', () => ({ resolve: (path: string) => path }));
vi.mock('$app/navigation', () => ({ goto: vi.fn() }));
vi.mock('$app/state', () => ({ page: { url: new URL('http://localhost/login') } }));
vi.mock('$lib/api/auth', () => ({
	login: vi.fn(),
	register: vi.fn(),
	resendVerification: vi.fn(),
	fetchAuthOptions: vi.fn(),
	startOAuth: vi.fn()
}));
vi.mock('$lib/stores/auth.svelte', () => ({
	initAuth: vi.fn().mockResolvedValue(undefined),
	deactivateAccountLocally: vi.fn().mockResolvedValue(undefined)
}));

const children = createRawSnippet(() => ({
	render: () => '<div data-testid="live-content">live</div>'
}));

describe('offline-unavailable states', () => {
	beforeEach(async () => {
		connection.online = false;
		const { fetchAuthOptions } = await import('$lib/api/auth');
		vi.mocked(fetchAuthOptions).mockResolvedValue({
			privacy_policy: { version: 'test-v1', url: '/privacy' },
			oauth: { google: true, github: true }
		});
	});

	it('OfflineNotice explains the missing connection only while offline', () => {
		const { unmount } = render(OfflineNotice, { props: { feature: 'Die Anmeldung' } });
		expect(screen.getByTestId('offline-unavailable')).toHaveTextContent(
			'Die Anmeldung benötigt eine Internetverbindung'
		);
		unmount();

		connection.online = true;
		render(OfflineNotice, { props: { feature: 'Die Anmeldung' } });
		expect(screen.queryByTestId('offline-unavailable')).not.toBeInTheDocument();
	});

	it('OnlineOnly replaces live content with a notice and a link to the offline collection', () => {
		render(OnlineOnly, { props: { feature: 'Tauschabgleich', children } });

		expect(screen.queryByTestId('live-content')).not.toBeInTheDocument();
		expect(screen.getByTestId('offline-unavailable-page')).toHaveTextContent(
			'Tauschabgleich benötigt eine Internetverbindung'
		);
		expect(screen.getByTestId('offline-collection-link')).toHaveAttribute('href', '/collection');
	});

	it('OnlineOnly renders the live content when online', () => {
		connection.online = true;
		render(OnlineOnly, { props: { feature: 'Tauschabgleich', children } });

		expect(screen.getByTestId('live-content')).toBeInTheDocument();
		expect(screen.queryByTestId('offline-unavailable-page')).not.toBeInTheDocument();
	});

	it.each([
		['trades', TradesLayout, 'Tauschabgleich'],
		['messages', MessagesLayout, 'Nachrichten']
	])('the %s section shows the offline state instead of failing ambiguously', (_, Layout, name) => {
		render(Layout, { props: { children } });

		expect(screen.queryByTestId('live-content')).not.toBeInTheDocument();
		expect(screen.getByTestId('offline-unavailable-page')).toHaveTextContent(name);
	});

	it.each([
		['login', LoginPage],
		['register', RegisterPage]
	])('the %s page disables OAuth and says why while offline', async (_, Page) => {
		render(Page);

		expect(screen.getByTestId('offline-unavailable')).toBeInTheDocument();
		await waitFor(() => {
			expect(screen.getByTestId('oauth-google')).toBeDisabled();
			expect(screen.getByTestId('oauth-github')).toBeDisabled();
		});
	});

	it('the login page enables OAuth again when online', async () => {
		connection.online = true;
		render(LoginPage);

		await waitFor(() => expect(screen.getByTestId('oauth-google')).toBeEnabled());
		expect(screen.queryByTestId('offline-unavailable')).not.toBeInTheDocument();
	});
});
