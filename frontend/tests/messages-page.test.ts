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
});
