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
	completeTrade: vi.fn(),
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
	cancelTrade: (...args: unknown[]) => mocks.cancelTrade(...args),
	completeTrade: (...args: unknown[]) => mocks.completeTrade(...args)
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
	edition_label: null,
	wanted_edition_label: null,
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
	completed_at: null,
	my_completion_confirmed_at: null,
	partner_completion_confirmed_at: null,
	updated_at: '2026-08-10T08:00:00Z'
};

const acceptedTrade = {
	...proposedTrade,
	status: 'accepted' as const,
	accepted_at: '2026-08-10T09:00:00Z'
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
			...acceptedTrade
		});
		mocks.cancelTrade.mockResolvedValue(undefined);
		mocks.completeTrade.mockResolvedValue({
			...acceptedTrade,
			my_completion_confirmed_at: '2026-08-11T08:00:00Z'
		});
		mocks.fetchMessages.mockResolvedValue({ data: [], next_before_id: null });
		mocks.markThreadRead.mockResolvedValue(undefined);
	});

	it('renders the immutable proposal snapshot and message thread', async () => {
		mocks.fetchTrade.mockResolvedValue({
			...proposedTrade,
			my_offers: [{ ...item, edition_label: '1. Auflage' }]
		});
		const view = render(TradeDetailPage);

		await waitFor(() => expect(screen.getByText('Tausch mit Mira')).toBeInTheDocument());
		expect(mocks.fetchTrade).toHaveBeenCalledWith(8);
		expect(screen.getByText('Maddrax #42: Dunkle Zukunft · Z2 · 1. Auflage')).toBeInTheDocument();
		expect(screen.getByText('Maddrax #7: Die Gruft · Z2')).toBeInTheDocument();
		expect(screen.getByTestId('message-thread')).toBeInTheDocument();
		view.unmount();
	});

	it('records the first completion confirmation and waits for the partner', async () => {
		mocks.fetchTrade.mockResolvedValueOnce(acceptedTrade);
		const view = render(TradeDetailPage);
		const user = userEvent.setup();

		await user.click(await screen.findByRole('button', { name: 'Tausch als erhalten bestätigen' }));

		await waitFor(() => expect(mocks.completeTrade).toHaveBeenCalledWith(8));
		expect(screen.getByTestId('completion-waiting')).toHaveTextContent(
			'Warten auf die Bestätigung der anderen Seite.'
		);
		expect(
			screen.queryByRole('button', { name: 'Tausch als erhalten bestätigen' })
		).not.toBeInTheDocument();
		view.unmount();
	});

	it('shows the atomic completion after the second confirmation', async () => {
		mocks.fetchTrade.mockResolvedValueOnce({
			...acceptedTrade,
			partner_completion_confirmed_at: '2026-08-11T08:00:00Z'
		});
		mocks.completeTrade.mockResolvedValueOnce({
			...acceptedTrade,
			status: 'completed',
			completed_at: '2026-08-11T09:00:00Z',
			my_completion_confirmed_at: '2026-08-11T09:00:00Z',
			partner_completion_confirmed_at: '2026-08-11T08:00:00Z'
		});
		const completed = render(TradeDetailPage);
		const user = userEvent.setup();

		await user.click(await screen.findByRole('button', { name: 'Tausch als erhalten bestätigen' }));

		expect(await screen.findByText('Abgeschlossen')).toBeInTheDocument();
		expect(mocks.completeTrade).toHaveBeenCalledWith(8);
		expect(screen.getByTestId('completion-finished')).toHaveTextContent('Abgeschlossen am');
		completed.unmount();
	});

	it('reports typed and fallback completion errors', async () => {
		mocks.fetchTrade.mockResolvedValue(acceptedTrade);
		mocks.completeTrade.mockRejectedValueOnce(new Error('Sammlung wurde verändert'));
		const typed = render(TradeDetailPage);
		const typedUser = userEvent.setup();
		await typedUser.click(
			await screen.findByRole('button', { name: 'Tausch als erhalten bestätigen' })
		);
		await waitFor(() =>
			expect(screen.getByRole('alert')).toHaveTextContent('Sammlung wurde verändert')
		);
		typed.unmount();

		mocks.completeTrade.mockRejectedValueOnce('offline');
		const fallback = render(TradeDetailPage);
		const fallbackUser = userEvent.setup();
		await fallbackUser.click(
			await screen.findByRole('button', { name: 'Tausch als erhalten bestätigen' })
		);
		await waitFor(() =>
			expect(screen.getByRole('alert')).toHaveTextContent(
				'Tauschabschluss konnte nicht bestätigt werden.'
			)
		);
		fallback.unmount();
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

	it('redirects anonymous users and rejects zero as an id', async () => {
		mocks.getAuthState.mockReturnValue({ isAuthenticated: false, isLoading: false, user: null });
		const anonymous = render(TradeDetailPage);
		await waitFor(() => expect(mocks.goto).toHaveBeenCalledWith('/login'));
		expect(mocks.fetchTrade).not.toHaveBeenCalled();
		anonymous.unmount();

		mocks.getAuthState.mockReturnValue(authState());
		mockPage.set({ params: { id: '0' } });
		const invalid = render(TradeDetailPage);
		await waitFor(() =>
			expect(screen.getByRole('alert')).toHaveTextContent('Ungültige Tausch-ID.')
		);
		expect(mocks.fetchTrade).not.toHaveBeenCalled();
		invalid.unmount();
	});

	it('waits for authentication initialization before loading or redirecting', () => {
		mocks.getAuthState.mockReturnValue({ isAuthenticated: false, isLoading: true, user: null });
		const view = render(TradeDetailPage);

		expect(screen.getByText('Tausch wird geladen …')).toBeInTheDocument();
		expect(mocks.fetchTrade).not.toHaveBeenCalled();
		expect(mocks.goto).not.toHaveBeenCalled();
		view.unmount();
	});

	it('shows its loading state and a typed load failure', async () => {
		mocks.fetchTrade.mockReturnValueOnce(new Promise(() => {}));
		const pending = render(TradeDetailPage);
		expect(screen.getByText('Tausch wird geladen …')).toBeInTheDocument();
		pending.unmount();

		mocks.fetchTrade.mockRejectedValueOnce(new Error('Tausch nicht verfügbar'));
		const failed = render(TradeDetailPage);
		await waitFor(() =>
			expect(screen.getByRole('alert')).toHaveTextContent('Tausch nicht verfügbar')
		);
		failed.unmount();
	});

	it('uses fallback action errors for accept and cancel', async () => {
		mocks.acceptTrade.mockRejectedValueOnce('offline');
		const acceptFailure = render(TradeDetailPage);
		const acceptUser = userEvent.setup();
		await acceptUser.click(await screen.findByRole('button', { name: 'Tausch annehmen' }));
		await waitFor(() =>
			expect(screen.getByRole('alert')).toHaveTextContent('Tausch konnte nicht angenommen werden.')
		);
		acceptFailure.unmount();

		mocks.cancelTrade.mockRejectedValueOnce('offline');
		const cancelFailure = render(TradeDetailPage);
		const cancelUser = userEvent.setup();
		await cancelUser.click(await screen.findByRole('button', { name: 'Tausch abbrechen' }));
		await waitFor(() =>
			expect(screen.getByRole('alert')).toHaveTextContent('Tausch konnte nicht abgebrochen werden.')
		);
		cancelFailure.unmount();
	});

	it('shows a typed cancel error', async () => {
		mocks.cancelTrade.mockRejectedValueOnce(new Error('Abbruch gesperrt'));
		const view = render(TradeDetailPage);
		const user = userEvent.setup();
		await user.click(await screen.findByRole('button', { name: 'Tausch abbrechen' }));

		await waitFor(() => expect(screen.getByRole('alert')).toHaveTextContent('Abbruch gesperrt'));
		view.unmount();
	});

	it('hides responder-only actions from initiators and closed trades', async () => {
		mocks.fetchTrade.mockResolvedValueOnce({ ...proposedTrade, role: 'initiator' });
		const initiator = render(TradeDetailPage);
		await waitFor(() => expect(screen.getByText('Vorgeschlagen')).toBeInTheDocument());
		expect(screen.queryByRole('button', { name: 'Tausch annehmen' })).not.toBeInTheDocument();
		expect(screen.getByRole('button', { name: 'Tausch abbrechen' })).toBeInTheDocument();
		initiator.unmount();

		mocks.fetchTrade.mockResolvedValueOnce({ ...proposedTrade, status: 'completed' });
		const completed = render(TradeDetailPage);
		await waitFor(() => expect(screen.getByText('Abgeschlossen')).toBeInTheDocument());
		expect(screen.queryByRole('button', { name: 'Tausch annehmen' })).not.toBeInTheDocument();
		expect(screen.queryByRole('button', { name: 'Tausch abbrechen' })).not.toBeInTheDocument();
		completed.unmount();
	});
});
