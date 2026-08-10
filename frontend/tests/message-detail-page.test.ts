import { beforeEach, describe, expect, it, vi } from 'vitest';
import { render, screen, waitFor } from '@testing-library/svelte';
import MessageDetailPage from '../src/routes/messages/[id]/+page.svelte';

function createMockStore<T>(initial: T) {
	let value = initial;
	const subscribers = new Set<(next: T) => void>();
	return {
		subscribe(subscriber: (next: T) => void) {
			subscribers.add(subscriber);
			subscriber(value);
			return () => subscribers.delete(subscriber);
		},
		set(next: T) {
			value = next;
			subscribers.forEach((subscriber) => subscriber(next));
		}
	};
}

const mockPage = createMockStore({ params: { id: '12' } });
const mocks = vi.hoisted(() => ({
	getAuthState: vi.fn(),
	fetchMessages: vi.fn(),
	markThreadRead: vi.fn(),
	sendMessage: vi.fn(),
	goto: vi.fn()
}));

vi.mock('$app/stores', () => ({
	page: { subscribe: (subscriber: (value: unknown) => void) => mockPage.subscribe(subscriber) }
}));
vi.mock('$app/navigation', () => ({ goto: (...args: unknown[]) => mocks.goto(...args) }));
vi.mock('$app/paths', () => ({ resolve: (path: string) => path }));
vi.mock('$lib/stores/auth.svelte', () => ({ getAuthState: () => mocks.getAuthState() }));
vi.mock('$lib/api/messages', () => ({
	fetchMessages: (...args: unknown[]) => mocks.fetchMessages(...args),
	markThreadRead: (...args: unknown[]) => mocks.markThreadRead(...args),
	sendMessage: (...args: unknown[]) => mocks.sendMessage(...args)
}));

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

describe('Message detail page', () => {
	beforeEach(() => {
		vi.clearAllMocks();
		mockPage.set({ params: { id: '12' } });
		mocks.getAuthState.mockReturnValue(authState());
		mocks.fetchMessages.mockResolvedValue({ data: [], next_before_id: null });
		mocks.markThreadRead.mockResolvedValue(undefined);
	});

	it('renders the selected thread and links back to the inbox', async () => {
		const view = render(MessageDetailPage);

		await waitFor(() => expect(screen.getByTestId('message-thread')).toBeInTheDocument());
		expect(mocks.fetchMessages).toHaveBeenCalledWith(12, { limit: 100 });
		expect(screen.getByRole('link', { name: '← Zurück zu Nachrichten' })).toHaveAttribute(
			'href',
			'/messages'
		);
		view.unmount();
	});

	it('rejects malformed thread IDs before rendering a thread', async () => {
		mockPage.set({ params: { id: 'invalid' } });
		render(MessageDetailPage);

		expect(screen.getByRole('alert')).toHaveTextContent('Ungültige Thread-ID.');
		expect(screen.queryByTestId('message-thread')).not.toBeInTheDocument();
		expect(mocks.fetchMessages).not.toHaveBeenCalled();
	});

	it('redirects unauthenticated users to login', async () => {
		mocks.getAuthState.mockReturnValue({ isAuthenticated: false, isLoading: false, user: null });
		const view = render(MessageDetailPage);

		await waitFor(() => expect(mocks.goto).toHaveBeenCalledWith('/login'));
		expect(screen.queryByTestId('message-thread')).not.toBeInTheDocument();
		expect(mocks.fetchMessages).not.toHaveBeenCalled();
		view.unmount();
	});

	it('shows an authentication loading state without requesting the thread', () => {
		mocks.getAuthState.mockReturnValue({ isAuthenticated: false, isLoading: true, user: null });
		const view = render(MessageDetailPage);

		expect(screen.getByTestId('message-auth-loading')).toHaveTextContent('Anmeldung wird geprüft');
		expect(screen.queryByTestId('message-thread')).not.toBeInTheDocument();
		expect(mocks.fetchMessages).not.toHaveBeenCalled();
		expect(mocks.goto).not.toHaveBeenCalled();
		view.unmount();
	});
});
