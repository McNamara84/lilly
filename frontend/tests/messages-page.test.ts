import { beforeEach, describe, expect, it, vi } from 'vitest';
import { render, screen, waitFor } from '@testing-library/svelte';
import MessagesPage from '../src/routes/messages/+page.svelte';

const mocks = vi.hoisted(() => ({
	getAuthState: vi.fn(),
	fetchMessageThreads: vi.fn(),
	goto: vi.fn()
}));

vi.mock('$lib/stores/auth.svelte', () => ({ getAuthState: () => mocks.getAuthState() }));
vi.mock('$lib/api/messages', () => ({
	fetchMessageThreads: (...args: unknown[]) => mocks.fetchMessageThreads(...args)
}));
vi.mock('$app/navigation', () => ({ goto: (...args: unknown[]) => mocks.goto(...args) }));
vi.mock('$app/paths', () => ({ resolve: (path: string) => path }));

const thread = {
	id: 12,
	trade_id: 8,
	trade_status: 'proposed',
	partner: { id: 2, display_name: 'Mira', avatar_path: null, location: null },
	last_message: 'Versand als BüWa?',
	last_message_at: '2026-08-10T08:00:00Z',
	unread_count: 2,
	updated_at: '2026-08-10T08:00:00Z'
};

function authState() {
	return {
		isAuthenticated: true,
		isLoading: false,
		user: {
			id: 1,
			email: 'collector@example.com',
			display_name: 'Sammler',
			email_verified: true,
			role: 'user' as const
		}
	};
}

describe('Messages page', () => {
	beforeEach(() => {
		vi.clearAllMocks();
		mocks.getAuthState.mockReturnValue(authState());
		mocks.fetchMessageThreads.mockResolvedValue({
			data: [thread],
			page: 1,
			per_page: 100,
			total: 1
		});
	});

	it('lists trade conversations with previews and unread counts', async () => {
		render(MessagesPage);

		await waitFor(() => expect(screen.getByTestId('message-thread-list')).toBeInTheDocument());
		expect(mocks.fetchMessageThreads).toHaveBeenCalledWith({ per_page: 100 });
		expect(screen.getByText('Mira')).toBeInTheDocument();
		expect(screen.getByText('Versand als BüWa?')).toBeInTheDocument();
		expect(screen.getByText('2')).toBeInTheDocument();
		expect(screen.getByRole('link', { name: /Mira/ })).toHaveAttribute('href', '/messages/12');
	});

	it('renders empty and error states', async () => {
		mocks.fetchMessageThreads.mockResolvedValueOnce({ data: [], page: 1, per_page: 100, total: 0 });
		const empty = render(MessagesPage);
		await waitFor(() => expect(screen.getByText('Noch keine Unterhaltungen.')).toBeInTheDocument());
		empty.unmount();

		mocks.fetchMessageThreads.mockRejectedValueOnce('offline');
		render(MessagesPage);
		await waitFor(() =>
			expect(screen.getByRole('alert')).toHaveTextContent(
				'Nachrichten konnten nicht geladen werden.'
			)
		);
	});

	it('redirects unauthenticated users without loading the inbox', async () => {
		mocks.getAuthState.mockReturnValue({ isAuthenticated: false, isLoading: false, user: null });
		render(MessagesPage);

		await waitFor(() => expect(mocks.goto).toHaveBeenCalledWith('/login'));
		expect(mocks.fetchMessageThreads).not.toHaveBeenCalled();
	});

	it('waits for authentication initialization before loading or redirecting', () => {
		mocks.getAuthState.mockReturnValue({ isAuthenticated: false, isLoading: true, user: null });
		const view = render(MessagesPage);

		expect(screen.getByText('Nachrichten werden geladen …')).toBeInTheDocument();
		expect(mocks.fetchMessageThreads).not.toHaveBeenCalled();
		expect(mocks.goto).not.toHaveBeenCalled();
		view.unmount();
	});

	it('shows the loading state while the inbox request is pending', () => {
		mocks.fetchMessageThreads.mockReturnValue(new Promise(() => {}));
		const view = render(MessagesPage);

		expect(screen.getByText('Nachrichten werden geladen …')).toBeInTheDocument();
		view.unmount();
	});

	it('renders threads without messages or unread entries', async () => {
		mocks.fetchMessageThreads.mockResolvedValue({
			data: [{ ...thread, last_message: null, last_message_at: null, unread_count: 0 }],
			page: 1,
			per_page: 100,
			total: 1
		});
		const view = render(MessagesPage);

		await waitFor(() => expect(screen.getByText('Noch keine Nachricht')).toBeInTheDocument());
		expect(screen.queryByText('0')).not.toBeInTheDocument();
		expect(
			screen.getByText(new Date(thread.updated_at).toLocaleDateString('de-DE'))
		).toBeInTheDocument();
		view.unmount();
	});

	it('shows a typed inbox error', async () => {
		mocks.fetchMessageThreads.mockRejectedValueOnce(new Error('Postfach gesperrt'));
		const view = render(MessagesPage);

		await waitFor(() => expect(screen.getByRole('alert')).toHaveTextContent('Postfach gesperrt'));
		view.unmount();
	});
});
