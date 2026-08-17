import { beforeEach, describe, expect, it, vi } from 'vitest';
import { render, screen, waitFor } from '@testing-library/svelte';
import { userEvent } from '@testing-library/user-event';
import AccountDeletionPage from '../src/routes/account/deletion/+page.svelte';

const mockFetchStatus = vi.fn();
const mockCancelDeletion = vi.fn();
const mockInitAuth = vi.fn();
const mockGoto = vi.fn();

vi.mock('$lib/api/account-erasure', () => ({
	fetchAccountDeletionStatus: (...args: unknown[]) => mockFetchStatus(...args),
	cancelAccountDeletion: (...args: unknown[]) => mockCancelDeletion(...args)
}));

vi.mock('$lib/stores/auth.svelte', () => ({
	initAuth: (...args: unknown[]) => mockInitAuth(...args)
}));

vi.mock('$app/navigation', () => ({
	goto: (...args: unknown[]) => mockGoto(...args)
}));

vi.mock('$app/paths', () => ({ resolve: (path: string) => path }));

describe('Account deletion recovery page', () => {
	beforeEach(() => {
		vi.clearAllMocks();
		mockFetchStatus.mockResolvedValue({
			status: 'scheduled',
			requested_at: '2026-08-17T08:00:00Z',
			scheduled_for: '2026-08-24T08:00:00Z',
			can_cancel: true
		});
		mockCancelDeletion.mockResolvedValue({ message: 'cancelled' });
		mockInitAuth.mockResolvedValue(undefined);
		mockGoto.mockResolvedValue(undefined);
	});

	it('shows the deadline and restores a normal session when cancellation succeeds', async () => {
		render(AccountDeletionPage);
		const user = userEvent.setup();

		expect(await screen.findByText(/Dein Konto ist deaktiviert/)).toBeInTheDocument();
		expect(screen.getByText(/24\.8\.2026/)).toBeInTheDocument();
		await user.click(screen.getByTestId('cancel-account-deletion'));

		await waitFor(() => expect(mockCancelDeletion).toHaveBeenCalledOnce());
		expect(mockInitAuth).toHaveBeenCalledOnce();
		expect(mockGoto).toHaveBeenCalledWith('/profile?deletion_cancelled=true');
	});

	it('offers a fresh login when the recovery cookie is missing', async () => {
		mockFetchStatus.mockRejectedValue(new Error('Recovery required'));

		render(AccountDeletionPage);

		expect(await screen.findByText(/Wiederherstellungszugang fehlt/)).toBeInTheDocument();
		expect(screen.getByRole('link', { name: 'Zur Anmeldung' })).toHaveAttribute('href', '/login');
		expect(screen.getByRole('alert')).toHaveTextContent('Recovery required');
	});
});
