import { beforeEach, describe, expect, it, vi } from 'vitest';
import { act, fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { userEvent } from '@testing-library/user-event';
import MessageThread from '../src/lib/components/messages/MessageThread.svelte';

const mocks = vi.hoisted(() => ({
	fetchMessages: vi.fn(),
	markThreadRead: vi.fn(),
	sendMessage: vi.fn()
}));

vi.mock('$lib/api/messages', () => ({
	fetchMessages: (...args: unknown[]) => mocks.fetchMessages(...args),
	markThreadRead: (...args: unknown[]) => mocks.markThreadRead(...args),
	sendMessage: (...args: unknown[]) => mocks.sendMessage(...args)
}));

const incoming = {
	id: 1,
	thread_id: 7,
	sender_id: 2,
	content: '<b>Hallo</b> & willkommen',
	created_at: '2026-08-10T08:00:00Z',
	read_at: null,
	is_mine: false
};

const outgoing = {
	...incoming,
	id: 2,
	sender_id: 1,
	content: 'Gern!',
	is_mine: true
};

describe('MessageThread', () => {
	beforeEach(() => {
		vi.clearAllMocks();
		mocks.fetchMessages.mockResolvedValue({ data: [incoming], next_before_id: null });
		mocks.markThreadRead.mockResolvedValue(undefined);
		mocks.sendMessage.mockResolvedValue(outgoing);
	});

	it('renders messages as text and marks the latest incoming message read', async () => {
		const view = render(MessageThread, { threadId: 7 });

		await waitFor(() => expect(screen.getByText('<b>Hallo</b> & willkommen')).toBeInTheDocument());
		expect(document.querySelector('b')).toBeNull();
		expect(mocks.fetchMessages).toHaveBeenCalledWith(7, { limit: 100 });
		expect(mocks.markThreadRead).toHaveBeenCalledWith(7, 1);
		view.unmount();
	});

	it('trims, sends and appends a message without duplicating it', async () => {
		const view = render(MessageThread, { threadId: 7 });
		const user = userEvent.setup();
		await waitFor(() => expect(screen.getByText('<b>Hallo</b> & willkommen')).toBeInTheDocument());

		await user.type(screen.getByLabelText('Nachricht'), '  Gern!  ');
		await user.click(screen.getByRole('button', { name: 'Senden' }));

		await waitFor(() => expect(mocks.sendMessage).toHaveBeenCalledWith(7, 'Gern!'));
		expect(screen.getByText('Gern!')).toBeInTheDocument();
		expect(screen.getByLabelText('Nachricht')).toHaveValue('');
		expect(screen.getByText('Nachricht gesendet.')).toBeInTheDocument();
		view.unmount();
	});

	it('keeps the draft and reports load and send failures', async () => {
		mocks.fetchMessages.mockRejectedValueOnce(new Error('Laden fehlgeschlagen'));
		const loadFailure = render(MessageThread, { threadId: 7 });
		await waitFor(() =>
			expect(screen.getByRole('alert')).toHaveTextContent('Laden fehlgeschlagen')
		);
		loadFailure.unmount();

		mocks.fetchMessages.mockResolvedValue({ data: [], next_before_id: null });
		mocks.sendMessage.mockRejectedValueOnce('offline');
		const sendFailure = render(MessageThread, { threadId: 7 });
		const user = userEvent.setup();
		await waitFor(() => expect(screen.getByText(/Noch keine Nachrichten/)).toBeInTheDocument());
		await user.type(screen.getByLabelText('Nachricht'), 'Bitte melden');
		await user.click(screen.getByRole('button', { name: 'Senden' }));
		await waitFor(() =>
			expect(screen.getByRole('alert')).toHaveTextContent('Nachricht konnte nicht gesendet werden.')
		);
		expect(screen.getByLabelText('Nachricht')).toHaveValue('Bitte melden');
		sendFailure.unmount();
	});

	it('uses the fallback message for an untyped initial load error', async () => {
		mocks.fetchMessages.mockRejectedValueOnce('offline');
		const view = render(MessageThread, { threadId: 7 });

		await waitFor(() =>
			expect(screen.getByRole('alert')).toHaveTextContent(
				'Nachrichten konnten nicht geladen werden.'
			)
		);
		view.unmount();
	});

	it('ignores an empty form submission', async () => {
		mocks.fetchMessages.mockResolvedValue({ data: [], next_before_id: null });
		const view = render(MessageThread, { threadId: 7 });
		await waitFor(() => expect(screen.getByText(/Noch keine Nachrichten/)).toBeInTheDocument());

		await fireEvent.submit(document.querySelector('form') as HTMLFormElement);

		expect(mocks.sendMessage).not.toHaveBeenCalled();
		view.unmount();
	});

	it('does not mark outgoing-only histories and shows both delivery states', async () => {
		mocks.fetchMessages.mockResolvedValue({
			data: [
				outgoing,
				{ ...outgoing, id: 3, content: 'Schon gelesen', read_at: '2026-08-10T09:00:00Z' }
			],
			next_before_id: null
		});
		const view = render(MessageThread, { threadId: 7 });

		await waitFor(() => expect(screen.getByText('Schon gelesen')).toBeInTheDocument());
		expect(screen.getByText(/Gesendet/)).toBeInTheDocument();
		expect(screen.getByText(/Gelesen/)).toBeInTheDocument();
		expect(mocks.markThreadRead).not.toHaveBeenCalled();
		view.unmount();
	});

	it('does not mark an incoming message that is already read', async () => {
		mocks.fetchMessages.mockResolvedValue({
			data: [{ ...incoming, read_at: '2026-08-10T09:00:00Z' }],
			next_before_id: null
		});
		const view = render(MessageThread, { threadId: 7 });

		await waitFor(() => expect(screen.getByText('<b>Hallo</b> & willkommen')).toBeInTheDocument());
		expect(mocks.markThreadRead).not.toHaveBeenCalled();
		view.unmount();
	});

	it('loads and prepends older message pages through the cursor', async () => {
		const newest = { ...outgoing, id: 200, content: 'Neueste Nachricht' };
		const older = {
			...incoming,
			id: 50,
			content: 'Älteste Nachricht',
			read_at: '2026-08-10T09:00:00Z'
		};
		mocks.fetchMessages
			.mockResolvedValueOnce({ data: [newest], next_before_id: 100 })
			.mockResolvedValueOnce({ data: [older], next_before_id: null });
		const view = render(MessageThread, { threadId: 7 });
		const user = userEvent.setup();

		await user.click(await screen.findByRole('button', { name: 'Ältere Nachrichten laden' }));

		await waitFor(() =>
			expect(mocks.fetchMessages).toHaveBeenLastCalledWith(7, { before_id: 100, limit: 100 })
		);
		expect(screen.getByText('Älteste Nachricht')).toBeInTheDocument();
		expect(screen.getByText('Neueste Nachricht')).toBeInTheDocument();
		expect(
			screen.queryByRole('button', { name: 'Ältere Nachrichten laden' })
		).not.toBeInTheDocument();
		view.unmount();
	});

	it('counts Unicode code points consistently with the backend limit', async () => {
		mocks.fetchMessages.mockResolvedValue({ data: [], next_before_id: null });
		const view = render(MessageThread, { threadId: 7 });
		await waitFor(() => expect(screen.getByText(/Noch keine Nachrichten/)).toBeInTheDocument());
		const textarea = screen.getByLabelText('Nachricht');
		const sendButton = screen.getByRole('button', { name: 'Senden' });

		expect(textarea).not.toHaveAttribute('maxlength');
		await fireEvent.input(textarea, { target: { value: '😀'.repeat(4000) } });
		expect(screen.getByText('4000/4000')).toBeInTheDocument();
		expect(sendButton).toBeEnabled();

		await fireEvent.input(textarea, { target: { value: '😀'.repeat(4001) } });
		expect(screen.getByText('4001/4000')).toBeInTheDocument();
		expect(sendButton).toBeDisabled();
		view.unmount();
	});

	it('does not append a message already returned by the server', async () => {
		mocks.fetchMessages.mockResolvedValue({ data: [outgoing], next_before_id: null });
		mocks.sendMessage.mockResolvedValue(outgoing);
		const view = render(MessageThread, { threadId: 7 });
		const user = userEvent.setup();
		await waitFor(() => expect(screen.getAllByText('Gern!')).toHaveLength(1));

		await user.type(screen.getByLabelText('Nachricht'), 'Erneut senden');
		await user.click(screen.getByRole('button', { name: 'Senden' }));

		await waitFor(() => expect(mocks.sendMessage).toHaveBeenCalledWith(7, 'Erneut senden'));
		expect(screen.getAllByText('Gern!')).toHaveLength(1);
		view.unmount();
	});

	it('silently polls while visible and skips polling in a hidden document', async () => {
		vi.useFakeTimers();
		let hidden = false;
		const hiddenSpy = vi.spyOn(document, 'hidden', 'get').mockImplementation(() => hidden);
		mocks.fetchMessages
			.mockResolvedValueOnce({
				data: [{ ...incoming, read_at: '2026-08-10T09:00:00Z' }],
				next_before_id: null
			})
			.mockResolvedValueOnce({ data: [outgoing], next_before_id: null })
			.mockRejectedValueOnce(new Error('Temporärer Pollingfehler'));
		const view = render(MessageThread, { threadId: 7 });

		try {
			await act(async () => {
				await Promise.resolve();
			});
			expect(mocks.fetchMessages).toHaveBeenCalledTimes(1);

			await act(async () => {
				await vi.advanceTimersByTimeAsync(10_000);
			});
			expect(mocks.fetchMessages).toHaveBeenCalledTimes(2);
			expect(screen.getByText('<b>Hallo</b> & willkommen')).toBeInTheDocument();
			expect(screen.getByText('Gern!')).toBeInTheDocument();

			await act(async () => {
				await vi.advanceTimersByTimeAsync(10_000);
			});
			expect(mocks.fetchMessages).toHaveBeenCalledTimes(3);
			expect(screen.queryByRole('alert')).not.toBeInTheDocument();

			hidden = true;
			await act(async () => {
				await vi.advanceTimersByTimeAsync(10_000);
			});
			expect(mocks.fetchMessages).toHaveBeenCalledTimes(3);
		} finally {
			view.unmount();
			hiddenSpy.mockRestore();
			vi.useRealTimers();
		}
	});

	it('shows the typed send error', async () => {
		mocks.fetchMessages.mockResolvedValue({ data: [], next_before_id: null });
		mocks.sendMessage.mockRejectedValueOnce(new Error('Versand gesperrt'));
		const view = render(MessageThread, { threadId: 7 });
		const user = userEvent.setup();
		await waitFor(() => expect(screen.getByText(/Noch keine Nachrichten/)).toBeInTheDocument());

		await user.type(screen.getByLabelText('Nachricht'), 'Hallo');
		await user.click(screen.getByRole('button', { name: 'Senden' }));

		await waitFor(() => expect(screen.getByRole('alert')).toHaveTextContent('Versand gesperrt'));
		view.unmount();
	});
});
