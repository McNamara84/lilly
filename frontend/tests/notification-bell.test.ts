import { beforeEach, describe, expect, it, vi } from 'vitest';
import { act, render, screen, waitFor } from '@testing-library/svelte';
import { userEvent } from '@testing-library/user-event';
import NotificationBell from '../src/lib/components/notifications/NotificationBell.svelte';

const mocks = vi.hoisted(() => ({
	fetchNotifications: vi.fn(),
	fetchUnreadNotificationCount: vi.fn(),
	markAllNotificationsRead: vi.fn(),
	markNotificationRead: vi.fn(),
	goto: vi.fn()
}));

vi.mock('$lib/api/notifications', () => ({
	fetchNotifications: (...args: unknown[]) => mocks.fetchNotifications(...args),
	fetchUnreadNotificationCount: (...args: unknown[]) => mocks.fetchUnreadNotificationCount(...args),
	markAllNotificationsRead: (...args: unknown[]) => mocks.markAllNotificationsRead(...args),
	markNotificationRead: (...args: unknown[]) => mocks.markNotificationRead(...args)
}));
vi.mock('$app/navigation', () => ({ goto: (...args: unknown[]) => mocks.goto(...args) }));
vi.mock('$app/paths', () => ({
	resolve: (route: string, params?: { id: string }) =>
		params ? route.replace('[id]', params.id) : route
}));

const messageNotification = {
	id: 4,
	kind: 'trade_message' as const,
	actor_user_id: 2,
	match_id: 1,
	trade_id: 8,
	message_id: 13,
	payload: { thread_id: 7 },
	read_at: null,
	created_at: '2026-08-10T08:00:00Z'
};

describe('NotificationBell', () => {
	beforeEach(() => {
		vi.clearAllMocks();
		mocks.fetchUnreadNotificationCount.mockResolvedValue(1);
		mocks.fetchNotifications.mockResolvedValue({
			data: [messageNotification],
			page: 1,
			per_page: 10,
			total: 1
		});
		mocks.markNotificationRead.mockResolvedValue(undefined);
		mocks.markAllNotificationsRead.mockResolvedValue(undefined);
		mocks.goto.mockResolvedValue(undefined);
	});

	it('shows the unread count, loads the popover and opens a message thread', async () => {
		const view = render(NotificationBell);
		const user = userEvent.setup();
		await waitFor(() => expect(screen.getByTestId('notification-count')).toHaveTextContent('1'));

		await user.click(screen.getByRole('button', { name: 'Benachrichtigungen, 1 ungelesen' }));
		await waitFor(() => expect(screen.getByText('Neue Nachricht')).toBeInTheDocument());
		expect(mocks.fetchNotifications).toHaveBeenCalledWith({ per_page: 10 });
		await user.click(screen.getByRole('button', { name: /Neue Nachricht/ }));

		await waitFor(() => expect(mocks.markNotificationRead).toHaveBeenCalledWith(4));
		expect(mocks.goto).toHaveBeenCalledWith('/messages/7');
		expect(screen.queryByTestId('notification-count')).not.toBeInTheDocument();
		view.unmount();
	});

	it('marks every notification read', async () => {
		mocks.fetchNotifications.mockResolvedValue({
			data: [
				messageNotification,
				{ ...messageNotification, id: 5, read_at: '2026-08-10T09:00:00Z' }
			],
			page: 1,
			per_page: 10,
			total: 2
		});
		const view = render(NotificationBell);
		const user = userEvent.setup();
		await waitFor(() => expect(screen.getByTestId('notification-count')).toBeInTheDocument());
		await user.click(screen.getByRole('button', { name: /Benachrichtigungen/ }));
		await screen.findAllByText('Neue Nachricht');
		await user.click(screen.getByRole('button', { name: 'Alle gelesen' }));

		await waitFor(() => expect(mocks.markAllNotificationsRead).toHaveBeenCalledOnce());
		expect(screen.queryByTestId('notification-count')).not.toBeInTheDocument();
		view.unmount();
	});

	it('closes an open popover without loading it again', async () => {
		const view = render(NotificationBell);
		const user = userEvent.setup();
		await waitFor(() => expect(screen.getByTestId('notification-count')).toBeInTheDocument());

		const bell = screen.getByRole('button', { name: /Benachrichtigungen/ });
		await user.click(bell);
		await screen.findByTestId('notification-popover');
		await user.click(bell);

		expect(screen.queryByTestId('notification-popover')).not.toBeInTheDocument();
		expect(mocks.fetchNotifications).toHaveBeenCalledTimes(1);
		view.unmount();
	});

	it('caps the badge and reports popover loading errors', async () => {
		mocks.fetchUnreadNotificationCount.mockResolvedValue(120);
		mocks.fetchNotifications.mockRejectedValueOnce(new Error('Dienst nicht erreichbar'));
		const view = render(NotificationBell);
		const user = userEvent.setup();
		await waitFor(() => expect(screen.getByTestId('notification-count')).toHaveTextContent('99+'));
		await user.click(screen.getByRole('button', { name: /Benachrichtigungen/ }));
		await waitFor(() =>
			expect(screen.getByRole('alert')).toHaveTextContent('Dienst nicht erreichbar')
		);
		view.unmount();
	});

	it('uses the fallback message for an untyped popover error', async () => {
		mocks.fetchNotifications.mockRejectedValueOnce('offline');
		const view = render(NotificationBell);
		const user = userEvent.setup();
		await waitFor(() => expect(screen.getByTestId('notification-count')).toBeInTheDocument());

		await user.click(screen.getByRole('button', { name: /Benachrichtigungen/ }));
		await waitFor(() =>
			expect(screen.getByRole('alert')).toHaveTextContent(
				'Benachrichtigungen konnten nicht geladen werden.'
			)
		);
		view.unmount();
	});

	it('routes trade and match notifications without marking already read entries again', async () => {
		const tradeNotification = {
			...messageNotification,
			id: 5,
			kind: 'trade_accepted' as const,
			message_id: null,
			payload: {},
			read_at: '2026-08-10T09:00:00Z'
		};
		const matchNotification = {
			...messageNotification,
			id: 6,
			kind: 'trade_match' as const,
			trade_id: null,
			message_id: null,
			payload: {},
			read_at: '2026-08-10T09:00:00Z'
		};
		mocks.fetchNotifications.mockResolvedValue({
			data: [tradeNotification, matchNotification],
			page: 1,
			per_page: 10,
			total: 2
		});
		const view = render(NotificationBell);
		const user = userEvent.setup();
		await waitFor(() => expect(screen.getByTestId('notification-count')).toBeInTheDocument());

		await user.click(screen.getByRole('button', { name: /Benachrichtigungen/ }));
		await user.click(await screen.findByRole('button', { name: /Tauschvorschlag angenommen/ }));
		expect(mocks.goto).toHaveBeenLastCalledWith('/trades/8');
		expect(mocks.markNotificationRead).not.toHaveBeenCalled();

		await user.click(screen.getByRole('button', { name: /Benachrichtigungen/ }));
		await user.click(await screen.findByRole('button', { name: /Neuer Tausch-Match/ }));
		expect(mocks.goto).toHaveBeenLastCalledWith('/trades');
		view.unmount();
	});

	it('shows an empty state and tolerates unread-count refresh failures', async () => {
		mocks.fetchUnreadNotificationCount.mockRejectedValueOnce(new Error('Polling fehlgeschlagen'));
		mocks.fetchNotifications.mockResolvedValueOnce({ data: [], page: 1, per_page: 10, total: 0 });
		const view = render(NotificationBell);
		const user = userEvent.setup();

		await user.click(screen.getByRole('button', { name: 'Benachrichtigungen' }));
		await waitFor(() => expect(screen.getByText('Keine Benachrichtigungen.')).toBeInTheDocument());
		expect(screen.queryByRole('alert')).not.toBeInTheDocument();
		view.unmount();
	});

	it('refreshes on a visible interval and on focus, but not while hidden', async () => {
		vi.useFakeTimers();
		let hidden = false;
		const hiddenSpy = vi.spyOn(document, 'hidden', 'get').mockImplementation(() => hidden);
		const view = render(NotificationBell);

		try {
			await act(async () => {
				await Promise.resolve();
			});
			expect(mocks.fetchUnreadNotificationCount).toHaveBeenCalledTimes(1);

			await act(async () => {
				await vi.advanceTimersByTimeAsync(30_000);
			});
			expect(mocks.fetchUnreadNotificationCount).toHaveBeenCalledTimes(2);

			hidden = true;
			await act(async () => {
				await vi.advanceTimersByTimeAsync(30_000);
			});
			expect(mocks.fetchUnreadNotificationCount).toHaveBeenCalledTimes(2);

			await act(async () => {
				window.dispatchEvent(new Event('focus'));
				await Promise.resolve();
			});
			expect(mocks.fetchUnreadNotificationCount).toHaveBeenCalledTimes(3);
		} finally {
			view.unmount();
			hiddenSpy.mockRestore();
			vi.useRealTimers();
		}
	});

	it('renders every notification kind label', async () => {
		const kinds = [
			'trade_match',
			'trade_match_updated',
			'trade_proposed',
			'trade_accepted',
			'trade_cancelled',
			'trade_message'
		] as const;
		mocks.fetchNotifications.mockResolvedValue({
			data: kinds.map((kind, index) => ({
				...messageNotification,
				id: index + 10,
				kind,
				read_at: '2026-08-10T09:00:00Z'
			})),
			page: 1,
			per_page: 10,
			total: kinds.length
		});
		const view = render(NotificationBell);
		const user = userEvent.setup();
		await waitFor(() => expect(screen.getByTestId('notification-count')).toBeInTheDocument());

		await user.click(screen.getByRole('button', { name: /Benachrichtigungen/ }));

		expect(await screen.findByText('Neuer Tausch-Match')).toBeInTheDocument();
		expect(screen.getByText('Tausch-Match aktualisiert')).toBeInTheDocument();
		expect(screen.getByText('Neuer Tauschvorschlag')).toBeInTheDocument();
		expect(screen.getByText('Tauschvorschlag angenommen')).toBeInTheDocument();
		expect(screen.getByText('Tausch abgebrochen')).toBeInTheDocument();
		expect(screen.getByText('Neue Nachricht')).toBeInTheDocument();
		view.unmount();
	});
});
