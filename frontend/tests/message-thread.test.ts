import { beforeEach, describe, expect, it, vi } from 'vitest';
import { render, screen, waitFor } from '@testing-library/svelte';
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
});
