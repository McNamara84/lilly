import { beforeEach, describe, expect, it, vi } from 'vitest';
import { render, screen, waitFor } from '@testing-library/svelte';
import { userEvent } from '@testing-library/user-event';
import TradeDetailPage from '../src/routes/trades/[id]/+page.svelte';

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

const mockPage = createMockStore({ params: { id: '8' } });
const mocks = vi.hoisted(() => ({
	getAuthState: vi.fn(),
	fetchTrade: vi.fn(),
	acceptTrade: vi.fn(),
	cancelTrade: vi.fn(),
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
vi.mock('$lib/api/trades', () => ({
	fetchTrade: (...args: unknown[]) => mocks.fetchTrade(...args),
	acceptTrade: (...args: unknown[]) => mocks.acceptTrade(...args),
	cancelTrade: (...args: unknown[]) => mocks.cancelTrade(...args)
}));
vi.mock('$lib/api/messages', () => ({
	fetchMessages: (...args: unknown[]) => mocks.fetchMessages(...args),
	markThreadRead: (...args: unknown[]) => mocks.markThreadRead(...args),
	sendMessage: (...args: unknown[]) => mocks.sendMessage(...args)
}));

const item = {
	entry_id: 10,
	wanted_entry_id: 20,
	issue_id: 42,
	issue_number: 42,
	title: 'Dunkle Zukunft',
	series_id: 1,
	series_name: 'Maddrax',
	series_slug: 'maddrax',
	cover_url: null,
	cover_local_path: null,
	copy_number: 1,
	condition_grade: 'Z2' as const
};

const proposedTrade = {
	id: 8,
	match_id: 5,
	status: 'proposed' as const,
	role: 'responder' as const,
	partner: { id: 2, display_name: 'Mira', avatar_path: null, location: null },
	my_offers: [item],
	partner_offers: [{ ...item, entry_id: 11, issue_id: 7, issue_number: 7, title: 'Die Gruft' }],
	thread_id: 12,
	cancellation_reason: null,
	proposed_at: '2026-08-10T08:00:00Z',
	accepted_at: null,
	cancelled_at: null,
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

describe('Trade detail page', () => {
	beforeEach(() => {
		vi.clearAllMocks();
		mockPage.set({ params: { id: '8' } });
		mocks.getAuthState.mockReturnValue(authState());
		mocks.fetchTrade.mockResolvedValue({ ...proposedTrade });
		mocks.acceptTrade.mockResolvedValue({
			...proposedTrade,
			status: 'accepted',
			accepted_at: '2026-08-10T09:00:00Z'
		});
		mocks.cancelTrade.mockResolvedValue(undefined);
		mocks.fetchMessages.mockResolvedValue({ data: [], next_before_id: null });
		mocks.markThreadRead.mockResolvedValue(undefined);
	});

	it('renders the immutable proposal snapshot and message thread', async () => {
		const view = render(TradeDetailPage);

		await waitFor(() => expect(screen.getByText('Tausch mit Mira')).toBeInTheDocument());
		expect(mocks.fetchTrade).toHaveBeenCalledWith(8);
		expect(screen.getByText('Maddrax #42: Dunkle Zukunft · Z2')).toBeInTheDocument();
		expect(screen.getByText('Maddrax #7: Die Gruft · Z2')).toBeInTheDocument();
		expect(screen.getByTestId('message-thread')).toBeInTheDocument();
		view.unmount();
	});

	it('allows only the responder to accept and updates the status', async () => {
		const view = render(TradeDetailPage);
		const user = userEvent.setup();
		await waitFor(() =>
			expect(screen.getByRole('button', { name: 'Tausch annehmen' })).toBeInTheDocument()
		);

		await user.click(screen.getByRole('button', { name: 'Tausch annehmen' }));

		await waitFor(() => expect(mocks.acceptTrade).toHaveBeenCalledWith(8));
		expect(screen.getByText('Aktiv')).toBeInTheDocument();
		expect(screen.queryByRole('button', { name: 'Tausch annehmen' })).not.toBeInTheDocument();
		view.unmount();
	});

	it('cancels an open trade and retains the conversation', async () => {
		const view = render(TradeDetailPage);
		const user = userEvent.setup();
		await waitFor(() =>
			expect(screen.getByRole('button', { name: 'Tausch abbrechen' })).toBeInTheDocument()
		);
		await user.click(screen.getByRole('button', { name: 'Tausch abbrechen' }));

		await waitFor(() => expect(mocks.cancelTrade).toHaveBeenCalledWith(8));
		expect(screen.getByText('Abgebrochen')).toBeInTheDocument();
		expect(screen.getByTestId('message-thread')).toBeInTheDocument();
		view.unmount();
	});

	it('reports action, load and invalid-ID errors', async () => {
		mocks.acceptTrade.mockRejectedValueOnce(new Error('Bereits reserviert'));
		const actionFailure = render(TradeDetailPage);
		const user = userEvent.setup();
		await screen.findByRole('button', { name: 'Tausch annehmen' });
		await user.click(screen.getByRole('button', { name: 'Tausch annehmen' }));
		await waitFor(() => expect(screen.getByRole('alert')).toHaveTextContent('Bereits reserviert'));
		actionFailure.unmount();

		mocks.fetchTrade.mockRejectedValueOnce('offline');
		const loadFailure = render(TradeDetailPage);
		await waitFor(() =>
			expect(screen.getByRole('alert')).toHaveTextContent('Tausch konnte nicht geladen werden.')
		);
		loadFailure.unmount();

		mockPage.set({ params: { id: 'invalid' } });
		render(TradeDetailPage);
		await waitFor(() =>
			expect(screen.getByRole('alert')).toHaveTextContent('Ungültige Tausch-ID.')
		);
	});
});
