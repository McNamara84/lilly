import { beforeEach, describe, expect, it, vi } from 'vitest';
import { act, render, screen, waitFor } from '@testing-library/svelte';
import { userEvent } from '@testing-library/user-event';
import TradesPage from '../src/routes/trades/+page.svelte';
import OffersPage from '../src/routes/trades/offers/+page.svelte';
import WantedPage from '../src/routes/trades/wanted/+page.svelte';
import TradeMatchCard from '../src/lib/components/trade/TradeMatchCard.svelte';
import TradeSummaryCard from '../src/lib/components/trade/TradeSummaryCard.svelte';

const mocks = vi.hoisted(() => ({
	getAuthState: vi.fn(),
	fetchMatches: vi.fn(),
	fetchOpenTrades: vi.fn(),
	createTradeProposal: vi.fn(),
	fetchTradeOffers: vi.fn(),
	fetchWantedEntries: vi.fn(),
	deleteWantedEntry: vi.fn(),
	updateCollectionEntry: vi.fn(),
	goto: vi.fn()
}));

vi.mock('$lib/stores/auth.svelte', () => ({
	getAuthState: () => mocks.getAuthState()
}));

vi.mock('$lib/api/trades', () => ({
	fetchMatches: (...args: unknown[]) => mocks.fetchMatches(...args),
	fetchOpenTrades: (...args: unknown[]) => mocks.fetchOpenTrades(...args),
	createTradeProposal: (...args: unknown[]) => mocks.createTradeProposal(...args),
	fetchTradeOffers: (...args: unknown[]) => mocks.fetchTradeOffers(...args),
	fetchWantedEntries: (...args: unknown[]) => mocks.fetchWantedEntries(...args),
	deleteWantedEntry: (...args: unknown[]) => mocks.deleteWantedEntry(...args)
}));

vi.mock('$lib/api/collection', () => ({
	updateCollectionEntry: (...args: unknown[]) => mocks.updateCollectionEntry(...args)
}));

vi.mock('$app/navigation', () => ({ goto: (...args: unknown[]) => mocks.goto(...args) }));
vi.mock('$app/paths', () => ({ resolve: (path: string) => path }));

const partner = {
	id: 2,
	display_name: 'Tauschpartnerin',
	avatar_path: null,
	location: 'Berlin'
};

const myOffer = {
	entry_id: 10,
	wanted_entry_id: 90,
	issue_id: 42,
	issue_number: 42,
	title: 'Dunkle Zukunft',
	series_id: 1,
	series_name: 'Maddrax',
	series_slug: 'maddrax',
	cover_url: null,
	cover_local_path: null,
	copy_number: 2,
	condition_grade: 'Z2' as const
};

const partnerOffer = {
	...myOffer,
	entry_id: 20,
	wanted_entry_id: 91,
	issue_id: 7,
	issue_number: 7,
	title: 'Gesuchtes Heft',
	series_id: 2,
	series_name: 'John Sinclair',
	series_slug: 'john-sinclair',
	copy_number: 1,
	condition_grade: 'Z1' as const
};

const match = {
	id: 5,
	status: 'active' as const,
	revision: 1,
	changed_at: '2026-08-10T08:00:00Z',
	partner,
	my_offers: [myOffer],
	partner_offers: [partnerOffer],
	match_score: 88,
	open_trade_id: null,
	open_trade_status: null
};

const trade = {
	id: 8,
	match_id: 5,
	status: 'proposed' as const,
	role: 'initiator' as const,
	partner,
	my_offers: [myOffer],
	partner_offers: [partnerOffer],
	thread_id: 12,
	cancellation_reason: null,
	proposed_at: '2026-08-10T08:00:00Z',
	accepted_at: null,
	cancelled_at: null,
	updated_at: '2026-08-10T08:00:00Z'
};

const offer = {
	...myOffer,
	offering_user_id: 1,
	offering_user_display_name: 'Sammler'
};

const wanted = {
	...partnerOffer,
	condition_grade: null
};

function authenticatedState() {
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

function page<T>(data: T[]) {
	return { data, page: 1, per_page: 50, total: data.length };
}

describe('Trades hub', () => {
	beforeEach(() => {
		vi.clearAllMocks();
		mocks.getAuthState.mockReturnValue(authenticatedState());
		mocks.fetchMatches.mockResolvedValue(page([match]));
		mocks.fetchOpenTrades.mockResolvedValue(page([trade]));
		mocks.createTradeProposal.mockResolvedValue(trade);
	});

	it('loads matches and active trades and links to list management', async () => {
		render(TradesPage);

		await waitFor(() => expect(screen.getByTestId('trade-match-card')).toBeInTheDocument());
		expect(mocks.fetchMatches).toHaveBeenCalledWith({ per_page: 50 });
		expect(mocks.fetchOpenTrades).toHaveBeenCalledWith({ per_page: 50 });
		expect(screen.getByText('Tauschpartnerin')).toBeInTheDocument();
		expect(screen.getByLabelText('Match-Score 88 Prozent')).toBeInTheDocument();
		expect(screen.getByRole('link', { name: 'Tauschbare Hefte' })).toHaveAttribute(
			'href',
			'/trades/offers'
		);
		expect(screen.getByRole('link', { name: 'Wunschliste' })).toHaveAttribute(
			'href',
			'/trades/wanted'
		);
	});

	it('switches to active trades', async () => {
		render(TradesPage);
		const user = userEvent.setup();

		await waitFor(() => expect(screen.getByTestId('matches-panel')).toBeInTheDocument());
		expect(screen.queryByRole('tab')).not.toBeInTheDocument();
		await user.click(screen.getByTestId('active-trades-tab'));

		expect(screen.getByTestId('active-trades-tab')).toHaveAttribute('aria-pressed', 'true');
		expect(screen.getByTestId('matches-tab')).toHaveAttribute('aria-pressed', 'false');
		expect(screen.getByTestId('trade-summary-card')).toHaveTextContent(
			'Tausch mit Tauschpartnerin'
		);
		expect(screen.getByRole('link', { name: 'Details und Nachrichten' })).toHaveAttribute(
			'href',
			'/trades/8'
		);
	});

	it('moves a newly created proposal into the active-trades tab', async () => {
		mocks.fetchOpenTrades.mockResolvedValue(
			page([
				{ ...trade, id: 99, match_id: 99, partner: { ...partner, display_name: 'Andere Person' } }
			])
		);
		render(TradesPage);
		const user = userEvent.setup();

		await waitFor(() =>
			expect(screen.getByRole('button', { name: 'Tausch vorschlagen' })).toBeInTheDocument()
		);
		await user.click(screen.getByRole('button', { name: 'Tausch vorschlagen' }));

		await waitFor(() => expect(mocks.createTradeProposal).toHaveBeenCalledWith(5, [10], [20]));
		expect(screen.getByTestId('active-trades-panel')).toBeInTheDocument();
		expect(screen.getAllByTestId('trade-summary-card')).toHaveLength(2);
	});

	it('validates item selection and reports proposal failures', async () => {
		mocks.createTradeProposal.mockRejectedValueOnce(new Error('Vorschlag kollidiert'));
		render(TradesPage);
		const user = userEvent.setup();

		await screen.findByRole('button', { name: 'Tausch vorschlagen' });
		const [offeredCheckbox, requestedCheckbox] = screen.getAllByRole('checkbox');
		await user.click(offeredCheckbox);
		expect(screen.getByRole('button', { name: 'Tausch vorschlagen' })).toBeDisabled();
		await user.click(offeredCheckbox);
		await user.click(requestedCheckbox);
		expect(screen.getByRole('button', { name: 'Tausch vorschlagen' })).toBeDisabled();
		await user.click(requestedCheckbox);
		await user.click(screen.getByRole('button', { name: 'Tausch vorschlagen' }));

		await waitFor(() =>
			expect(screen.getByRole('alert')).toHaveTextContent('Vorschlag kollidiert')
		);
	});

	it('links an existing open match to its trade instead of proposing again', async () => {
		mocks.fetchMatches.mockResolvedValue(
			page([{ ...match, open_trade_id: 8, open_trade_status: 'proposed' as const }])
		);
		render(TradesPage);

		const link = await screen.findByRole('link', { name: 'Offenen Tausch ansehen' });
		expect(link).toHaveAttribute('href', '/trades/8');
		expect(screen.queryByRole('button', { name: 'Tausch vorschlagen' })).not.toBeInTheDocument();
	});

	it('renders empty states and API errors', async () => {
		mocks.fetchMatches.mockResolvedValue(page([]));
		mocks.fetchOpenTrades.mockResolvedValue(page([]));
		const empty = render(TradesPage);
		await waitFor(() =>
			expect(screen.getByText('Noch keine Tauschvorschläge')).toBeInTheDocument()
		);
		empty.unmount();

		mocks.fetchMatches.mockRejectedValueOnce(new Error('Matching nicht erreichbar'));
		render(TradesPage);
		await waitFor(() =>
			expect(screen.getByRole('alert')).toHaveTextContent('Matching nicht erreichbar')
		);
	});

	it('uses a fallback error and redirects anonymous users', async () => {
		mocks.fetchMatches.mockRejectedValueOnce('offline');
		const failed = render(TradesPage);
		await waitFor(() =>
			expect(screen.getByRole('alert')).toHaveTextContent(
				'Tauschdaten konnten nicht geladen werden.'
			)
		);
		failed.unmount();

		mocks.getAuthState.mockReturnValue({ isAuthenticated: false, isLoading: false, user: null });
		render(TradesPage);
		await waitFor(() => expect(mocks.goto).toHaveBeenCalledWith('/login'));
	});

	it('waits for authentication initialization before loading or redirecting', () => {
		mocks.getAuthState.mockReturnValue({ isAuthenticated: false, isLoading: true, user: null });
		const view = render(TradesPage);

		expect(screen.getByTestId('trades-loading')).toBeInTheDocument();
		expect(mocks.fetchMatches).not.toHaveBeenCalled();
		expect(mocks.fetchOpenTrades).not.toHaveBeenCalled();
		expect(mocks.goto).not.toHaveBeenCalled();
		view.unmount();
	});

	it('shows the loading state, an empty active-trades state and switches back to matches', async () => {
		let resolveMatches!: (value: {
			data: (typeof match)[];
			page: number;
			per_page: number;
			total: number;
		}) => void;
		let resolveTrades!: (value: {
			data: (typeof trade)[];
			page: number;
			per_page: number;
			total: number;
		}) => void;
		mocks.fetchMatches.mockReturnValue(
			new Promise((resolve) => {
				resolveMatches = resolve;
			})
		);
		mocks.fetchOpenTrades.mockReturnValue(
			new Promise((resolve) => {
				resolveTrades = resolve;
			})
		);
		const view = render(TradesPage);
		const user = userEvent.setup();

		expect(screen.getByTestId('trades-loading')).toBeInTheDocument();
		resolveMatches(page([match]));
		resolveTrades(page([]));
		await waitFor(() => expect(screen.getByTestId('matches-panel')).toBeInTheDocument());

		await user.click(screen.getByTestId('active-trades-tab'));
		expect(screen.getByText('Noch keine offenen Tausche')).toBeInTheDocument();
		await user.click(screen.getByTestId('matches-tab'));
		expect(screen.getByTestId('trade-match-card')).toBeInTheDocument();
		view.unmount();
	});
});

describe('Trade cards', () => {
	beforeEach(() => {
		vi.clearAllMocks();
		mocks.createTradeProposal.mockResolvedValue(trade);
	});

	it('renders avatar and both cover sources and proposes without a callback', async () => {
		const richMatch = {
			...match,
			partner: { ...partner, avatar_path: '/avatars/partner.jpg', location: null },
			my_offers: [
				{ ...myOffer, cover_local_path: '/covers/local.jpg', cover_url: '/covers/ignored.jpg' }
			],
			partner_offers: [
				{ ...partnerOffer, cover_local_path: null, cover_url: 'https://img.example/remote.jpg' }
			]
		};
		const view = render(TradeMatchCard, { match: richMatch });
		const user = userEvent.setup();

		expect(document.querySelector('img[src="/avatars/partner.jpg"]')).toBeInTheDocument();
		expect(document.querySelector('img[src="/covers/local.jpg"]')).toBeInTheDocument();
		expect(document.querySelector('img[src="https://img.example/remote.jpg"]')).toBeInTheDocument();
		expect(screen.queryByText('Berlin')).not.toBeInTheDocument();

		await user.click(screen.getByRole('button', { name: 'Tausch vorschlagen' }));
		await waitFor(() => expect(mocks.createTradeProposal).toHaveBeenCalledWith(5, [10], [20]));
		view.unmount();
	});

	it('shows the fallback proposal error', async () => {
		mocks.createTradeProposal.mockRejectedValueOnce('offline');
		const view = render(TradeMatchCard, { match });
		const user = userEvent.setup();

		await user.click(screen.getByRole('button', { name: 'Tausch vorschlagen' }));

		await waitFor(() =>
			expect(screen.getByRole('alert')).toHaveTextContent(
				'Tauschvorschlag konnte nicht erstellt werden.'
			)
		);
		view.unmount();
	});

	it('guards invalid and duplicate proposal submissions', async () => {
		let resolveProposal!: (value: typeof trade) => void;
		mocks.createTradeProposal.mockReturnValue(
			new Promise((resolve) => {
				resolveProposal = resolve;
			})
		);
		const view = render(TradeMatchCard, { match });
		const user = userEvent.setup();
		const button = screen.getByRole('button', { name: 'Tausch vorschlagen' });
		const [offeredCheckbox, requestedCheckbox] = screen.getAllByRole('checkbox');
		const dispatchClick = () =>
			button.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true }));

		await user.click(offeredCheckbox);
		dispatchClick();
		expect(mocks.createTradeProposal).not.toHaveBeenCalled();

		await user.click(offeredCheckbox);
		await user.click(requestedCheckbox);
		dispatchClick();
		expect(mocks.createTradeProposal).not.toHaveBeenCalled();

		await user.click(requestedCheckbox);
		dispatchClick();
		expect(mocks.createTradeProposal).toHaveBeenCalledTimes(1);
		dispatchClick();
		expect(mocks.createTradeProposal).toHaveBeenCalledTimes(1);

		await act(async () => resolveProposal(trade));
		view.unmount();
	});

	it('renders accepted and received proposal labels', () => {
		const accepted = render(TradeSummaryCard, {
			trade: { ...trade, status: 'accepted' as const }
		});
		expect(screen.getByText('Aktiv')).toBeInTheDocument();
		accepted.unmount();

		const received = render(TradeSummaryCard, {
			trade: { ...trade, role: 'responder' as const }
		});
		expect(screen.getByText('Vorschlag erhalten')).toBeInTheDocument();
		received.unmount();
	});
});

describe('Trade list management pages', () => {
	beforeEach(() => {
		vi.clearAllMocks();
		mocks.getAuthState.mockReturnValue(authenticatedState());
		mocks.fetchTradeOffers.mockResolvedValue(page([offer]));
		mocks.fetchWantedEntries.mockResolvedValue(page([wanted]));
		mocks.updateCollectionEntry.mockResolvedValue({});
		mocks.deleteWantedEntry.mockResolvedValue(undefined);
	});

	it('lists and deactivates an offered duplicate', async () => {
		render(OffersPage);
		const user = userEvent.setup();

		await waitFor(() =>
			expect(screen.getByTestId('offer-card')).toHaveTextContent('Dunkle Zukunft')
		);
		expect(mocks.fetchTradeOffers).toHaveBeenCalledWith({ per_page: 100 });
		await user.click(screen.getByRole('button', { name: 'Nicht mehr tauschbar' }));

		await waitFor(() =>
			expect(mocks.updateCollectionEntry).toHaveBeenCalledWith(10, { status: 'owned' })
		);
		expect(screen.getByText('Noch keine Tauschangebote.')).toBeInTheDocument();
	});

	it('lists and removes a wanted entry', async () => {
		render(WantedPage);
		const user = userEvent.setup();

		await waitFor(() =>
			expect(screen.getByTestId('wanted-card')).toHaveTextContent('Gesuchtes Heft')
		);
		expect(mocks.fetchWantedEntries).toHaveBeenCalledWith({ per_page: 100 });
		await user.click(screen.getByRole('button', { name: 'Entfernen' }));

		await waitFor(() => expect(mocks.deleteWantedEntry).toHaveBeenCalledWith(20));
		expect(screen.getByText('Deine Wunschliste ist leer.')).toBeInTheDocument();
	});

	it('keeps entries visible and reports mutation failures', async () => {
		mocks.updateCollectionEntry.mockRejectedValueOnce(new Error('Reserviert'));
		render(OffersPage);
		const user = userEvent.setup();
		await waitFor(() => expect(screen.getByTestId('offer-card')).toBeInTheDocument());
		await user.click(screen.getByRole('button', { name: 'Nicht mehr tauschbar' }));
		await waitFor(() => expect(screen.getByRole('alert')).toHaveTextContent('Reserviert'));
		expect(screen.getByTestId('offer-card')).toBeInTheDocument();
	});

	it('shows list and fallback mutation errors', async () => {
		mocks.fetchWantedEntries.mockRejectedValueOnce(new Error('Listenfehler'));
		const failedList = render(WantedPage);
		await waitFor(() => expect(screen.getByRole('alert')).toHaveTextContent('Listenfehler'));
		failedList.unmount();

		mocks.fetchWantedEntries.mockResolvedValue(page([wanted]));
		mocks.deleteWantedEntry.mockRejectedValueOnce('untyped');
		render(WantedPage);
		const user = userEvent.setup();
		await waitFor(() => expect(screen.getByTestId('wanted-card')).toBeInTheDocument());
		await user.click(screen.getByRole('button', { name: 'Entfernen' }));
		await waitFor(() =>
			expect(screen.getByRole('alert')).toHaveTextContent('Wunsch konnte nicht entfernt werden.')
		);
	});

	it('shows the loading states while both lists are pending', () => {
		mocks.fetchTradeOffers.mockReturnValue(new Promise(() => {}));
		const offersView = render(OffersPage);
		expect(screen.getByText('Wird geladen …')).toBeInTheDocument();
		offersView.unmount();

		mocks.fetchWantedEntries.mockReturnValue(new Promise(() => {}));
		const wantedView = render(WantedPage);
		expect(screen.getByText('Wird geladen …')).toBeInTheDocument();
		wantedView.unmount();
	});

	it('redirects anonymous list visitors without loading data', async () => {
		mocks.getAuthState.mockReturnValue({ isAuthenticated: false, isLoading: false, user: null });
		const offersView = render(OffersPage);
		await waitFor(() => expect(mocks.goto).toHaveBeenCalledWith('/login'));
		expect(mocks.fetchTradeOffers).not.toHaveBeenCalled();
		offersView.unmount();

		vi.clearAllMocks();
		mocks.getAuthState.mockReturnValue({ isAuthenticated: false, isLoading: false, user: null });
		const wantedView = render(WantedPage);
		await waitFor(() => expect(mocks.goto).toHaveBeenCalledWith('/login'));
		expect(mocks.fetchWantedEntries).not.toHaveBeenCalled();
		wantedView.unmount();
	});

	it('waits for authentication initialization on both list pages', () => {
		mocks.getAuthState.mockReturnValue({ isAuthenticated: false, isLoading: true, user: null });
		const offersView = render(OffersPage);
		expect(screen.getByText('Wird geladen …')).toBeInTheDocument();
		expect(mocks.fetchTradeOffers).not.toHaveBeenCalled();
		expect(mocks.goto).not.toHaveBeenCalled();
		offersView.unmount();

		const wantedView = render(WantedPage);
		expect(screen.getByText('Wird geladen …')).toBeInTheDocument();
		expect(mocks.fetchWantedEntries).not.toHaveBeenCalled();
		expect(mocks.goto).not.toHaveBeenCalled();
		wantedView.unmount();
	});

	it('covers the complementary list and mutation error variants', async () => {
		mocks.fetchTradeOffers.mockRejectedValueOnce('offline');
		const failedOffers = render(OffersPage);
		await waitFor(() =>
			expect(screen.getByRole('alert')).toHaveTextContent(
				'Tauschangebote konnten nicht geladen werden.'
			)
		);
		failedOffers.unmount();

		mocks.fetchTradeOffers.mockResolvedValue(page([offer]));
		mocks.updateCollectionEntry.mockRejectedValueOnce('offline');
		const offerMutation = render(OffersPage);
		const offerUser = userEvent.setup();
		await waitFor(() => expect(screen.getByTestId('offer-card')).toBeInTheDocument());
		await offerUser.click(screen.getByRole('button', { name: 'Nicht mehr tauschbar' }));
		await waitFor(() =>
			expect(screen.getByRole('alert')).toHaveTextContent('Angebot konnte nicht entfernt werden.')
		);
		offerMutation.unmount();

		mocks.fetchWantedEntries.mockRejectedValueOnce('offline');
		const failedWanted = render(WantedPage);
		await waitFor(() =>
			expect(screen.getByRole('alert')).toHaveTextContent(
				'Wunschliste konnte nicht geladen werden.'
			)
		);
		failedWanted.unmount();

		mocks.fetchWantedEntries.mockResolvedValue(page([wanted]));
		mocks.deleteWantedEntry.mockRejectedValueOnce(new Error('Noch reserviert'));
		const wantedMutation = render(WantedPage);
		const wantedUser = userEvent.setup();
		await waitFor(() => expect(screen.getByTestId('wanted-card')).toBeInTheDocument());
		await wantedUser.click(screen.getByRole('button', { name: 'Entfernen' }));
		await waitFor(() => expect(screen.getByRole('alert')).toHaveTextContent('Noch reserviert'));
		wantedMutation.unmount();
	});

	it('shows a typed offer-list error', async () => {
		mocks.fetchTradeOffers.mockRejectedValueOnce(new Error('Angebotsliste gesperrt'));
		const view = render(OffersPage);

		await waitFor(() =>
			expect(screen.getByRole('alert')).toHaveTextContent('Angebotsliste gesperrt')
		);
		view.unmount();
	});
});
