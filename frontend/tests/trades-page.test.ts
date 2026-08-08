import { beforeEach, describe, expect, it, vi } from 'vitest';
import { render, screen, waitFor } from '@testing-library/svelte';
import { userEvent } from '@testing-library/user-event';
import TradesPage from '../src/routes/trades/+page.svelte';

const mocks = vi.hoisted(() => ({
	getAuthState: vi.fn(),
	fetchTradeOffers: vi.fn(),
	fetchWantedEntries: vi.fn(),
	deleteWantedEntry: vi.fn(),
	updateCollectionEntry: vi.fn()
}));

vi.mock('$lib/stores/auth.svelte', () => ({
	getAuthState: () => mocks.getAuthState()
}));

vi.mock('$lib/api/trades', () => ({
	fetchTradeOffers: (...args: unknown[]) => mocks.fetchTradeOffers(...args),
	fetchWantedEntries: (...args: unknown[]) => mocks.fetchWantedEntries(...args),
	deleteWantedEntry: (...args: unknown[]) => mocks.deleteWantedEntry(...args)
}));

vi.mock('$lib/api/collection', () => ({
	updateCollectionEntry: (...args: unknown[]) => mocks.updateCollectionEntry(...args)
}));

vi.mock('$app/navigation', () => ({ goto: vi.fn() }));
vi.mock('$app/paths', () => ({ resolve: (path: string) => path }));

const offer = {
	entry_id: 10,
	issue_id: 42,
	issue_number: 42,
	title: 'Dunkle Zukunft',
	series_id: 1,
	series_name: 'Maddrax',
	series_slug: 'maddrax',
	cover_url: 'https://example.com/42.jpg',
	cover_local_path: null,
	copy_number: 2,
	condition_grade: 'Z2',
	offering_user_id: 1,
	offering_user_display_name: 'Sammler'
};

const wanted = {
	entry_id: 20,
	issue_id: 7,
	issue_number: 7,
	title: 'Gesuchtes Heft',
	series_id: 2,
	series_name: 'John Sinclair',
	series_slug: 'john-sinclair',
	cover_url: null,
	cover_local_path: null,
	copy_number: 1,
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

describe('Trades page', () => {
	beforeEach(() => {
		vi.clearAllMocks();
		mocks.getAuthState.mockReturnValue(authenticatedState());
		mocks.fetchTradeOffers.mockResolvedValue({
			data: [offer],
			page: 1,
			per_page: 24,
			total: 1
		});
		mocks.fetchWantedEntries.mockResolvedValue({
			data: [wanted],
			page: 1,
			per_page: 24,
			total: 1
		});
		mocks.updateCollectionEntry.mockResolvedValue({});
		mocks.deleteWantedEntry.mockResolvedValue(undefined);
	});

	it('loads both private lists and renders the active offers tab', async () => {
		render(TradesPage);

		await waitFor(() => expect(screen.getByTestId('offer-card')).toBeInTheDocument());
		expect(mocks.fetchTradeOffers).toHaveBeenCalledWith({ page: 1, per_page: 24 });
		expect(mocks.fetchWantedEntries).toHaveBeenCalledWith({ page: 1, per_page: 24 });
		expect(screen.getByText('Dunkle Zukunft')).toBeInTheDocument();
		expect(screen.getByText('Zustand Z2')).toBeInTheDocument();
		expect(screen.getByText('Exemplar 2')).toBeInTheDocument();
		expect(screen.getByTestId('add-wanted-link')).toHaveAttribute('href', '/trades/wanted/add');
	});

	it('switches to the wanted tab and removes an entry', async () => {
		render(TradesPage);
		const user = userEvent.setup();

		await waitFor(() => expect(screen.getByTestId('offers-list')).toBeInTheDocument());
		await user.click(screen.getByTestId('wanted-tab'));
		expect(screen.getByTestId('wanted-card')).toHaveTextContent('Gesuchtes Heft');

		await user.click(screen.getByRole('button', { name: 'Entfernen' }));

		await waitFor(() => expect(mocks.deleteWantedEntry).toHaveBeenCalledWith(20));
		expect(screen.getByTestId('wanted-empty')).toBeInTheDocument();
		expect(screen.getByText(/wurde von der Wunschliste entfernt/)).toBeInTheDocument();
	});

	it('deactivates an offer by changing the existing entry to owned', async () => {
		render(TradesPage);
		const user = userEvent.setup();

		await waitFor(() => expect(screen.getByTestId('offer-card')).toBeInTheDocument());
		await user.click(screen.getByRole('button', { name: 'Nicht mehr tauschbar' }));

		await waitFor(() =>
			expect(mocks.updateCollectionEntry).toHaveBeenCalledWith(10, { status: 'owned' })
		);
		expect(screen.getByTestId('offers-empty')).toBeInTheDocument();
		expect(screen.getByText(/ist nicht mehr tauschbar/)).toBeInTheDocument();
	});

	it('loads the next offer page', async () => {
		mocks.fetchTradeOffers
			.mockResolvedValueOnce({ data: [offer], page: 1, per_page: 24, total: 25 })
			.mockResolvedValueOnce({
				data: [{ ...offer, entry_id: 11, issue_id: 43, issue_number: 43, title: 'Seite 2' }],
				page: 2,
				per_page: 24,
				total: 25
			});
		render(TradesPage);
		const user = userEvent.setup();

		await waitFor(() => expect(screen.getByRole('button', { name: 'Weiter' })).toBeInTheDocument());
		await user.click(screen.getByRole('button', { name: 'Weiter' }));

		await waitFor(() => expect(screen.getByText('Seite 2')).toBeInTheDocument());
		expect(mocks.fetchTradeOffers).toHaveBeenLastCalledWith({ page: 2, per_page: 24 });
	});

	it('shows API and mutation errors without removing entries', async () => {
		mocks.fetchTradeOffers.mockRejectedValueOnce(new Error('Angebotsfehler'));
		render(TradesPage);

		await waitFor(() => expect(screen.getByRole('alert')).toHaveTextContent('Angebotsfehler'));
	});

	it('keeps a wanted entry when deletion fails', async () => {
		mocks.deleteWantedEntry.mockRejectedValueOnce(new Error('Löschfehler'));
		render(TradesPage);
		const user = userEvent.setup();

		await waitFor(() => expect(screen.getByTestId('offers-list')).toBeInTheDocument());
		await user.click(screen.getByTestId('wanted-tab'));
		await user.click(screen.getByRole('button', { name: 'Entfernen' }));

		await waitFor(() => expect(screen.getByRole('alert')).toHaveTextContent('Löschfehler'));
		expect(screen.getByTestId('wanted-card')).toBeInTheDocument();
	});

	it('redirects unauthenticated users without loading private lists', async () => {
		const { goto } = await import('$app/navigation');
		mocks.getAuthState.mockReturnValue({ isAuthenticated: false, isLoading: false, user: null });

		render(TradesPage);

		await waitFor(() => expect(goto).toHaveBeenCalledWith('/login'));
		expect(mocks.fetchTradeOffers).not.toHaveBeenCalled();
		expect(mocks.fetchWantedEntries).not.toHaveBeenCalled();
	});
});
