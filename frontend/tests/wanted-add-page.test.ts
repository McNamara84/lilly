import { beforeEach, describe, expect, it, vi } from 'vitest';
import { render, screen, waitFor } from '@testing-library/svelte';
import { userEvent } from '@testing-library/user-event';
import WantedAddPage from '../src/routes/trades/wanted/add/+page.svelte';

const mocks = vi.hoisted(() => ({
	getAuthState: vi.fn(),
	fetchSeries: vi.fn(),
	fetchWantedCandidates: vi.fn(),
	addWantedBulk: vi.fn()
}));

vi.mock('$lib/stores/auth.svelte', () => ({
	getAuthState: () => mocks.getAuthState()
}));

vi.mock('$lib/api/series', () => ({
	fetchSeries: (...args: unknown[]) => mocks.fetchSeries(...args)
}));

vi.mock('$lib/api/trades', () => ({
	fetchWantedCandidates: (...args: unknown[]) => mocks.fetchWantedCandidates(...args),
	addWantedBulk: (...args: unknown[]) => mocks.addWantedBulk(...args)
}));

vi.mock('$app/navigation', () => ({ goto: vi.fn() }));
vi.mock('$app/paths', () => ({ resolve: (path: string) => path }));

const series = {
	id: 1,
	name: 'Maddrax',
	slug: 'maddrax',
	publisher: 'Bastei',
	genre: null,
	frequency: null,
	total_issues: 2,
	status: 'ongoing',
	active: true,
	source_url: null
};

const newCandidate = {
	issue_id: 1,
	issue_number: 1,
	title: 'Neuer Wunsch',
	series_id: 1,
	series_name: 'Maddrax',
	series_slug: 'maddrax',
	cover_url: null,
	cover_local_path: null,
	is_wanted: false,
	wanted_entry_id: null
};

const existingCandidate = {
	...newCandidate,
	issue_id: 2,
	issue_number: 2,
	title: 'Bereits gesucht',
	is_wanted: true,
	wanted_entry_id: 22
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

function deferred<T>() {
	let resolve!: (value: T | PromiseLike<T>) => void;
	const promise = new Promise<T>((complete) => {
		resolve = complete;
	});
	return { promise, resolve };
}

describe('Wanted add page', () => {
	beforeEach(() => {
		vi.clearAllMocks();
		mocks.getAuthState.mockReturnValue(authenticatedState());
		mocks.fetchSeries.mockResolvedValue([series]);
		mocks.fetchWantedCandidates.mockResolvedValue({
			data: [newCandidate, existingCandidate],
			page: 1,
			per_page: 50,
			total: 2
		});
		mocks.addWantedBulk.mockResolvedValue({
			created: [{ issue_id: 1, entry_id: 11 }],
			unchanged: [],
			rejected: []
		});
	});

	it('loads series, requires a selection and shows wanted state per candidate', async () => {
		render(WantedAddPage);
		const user = userEvent.setup();

		await waitFor(() => expect(screen.getByTestId('series-select')).toBeInTheDocument());
		expect(mocks.fetchWantedCandidates).not.toHaveBeenCalled();

		await user.selectOptions(screen.getByLabelText('Serie'), 'maddrax');

		await waitFor(() => expect(screen.getAllByTestId('candidate-item')).toHaveLength(2));
		expect(mocks.fetchWantedCandidates).toHaveBeenCalledWith({
			series_slug: 'maddrax',
			q: undefined,
			page: 1,
			per_page: 50
		});
		expect(screen.getByRole('checkbox', { name: /Bereits gesucht/ })).toBeDisabled();
		expect(screen.getByText('Bereits auf der Wunschliste')).toBeInTheDocument();
	});

	it('clears candidates and selection when the series is deselected', async () => {
		render(WantedAddPage);
		const user = userEvent.setup();

		await waitFor(() => expect(screen.getByTestId('series-select')).toBeInTheDocument());
		await user.selectOptions(screen.getByLabelText('Serie'), 'maddrax');
		await waitFor(() => expect(screen.getByTestId('candidate-list')).toBeInTheDocument());
		await user.click(screen.getByRole('checkbox', { name: /Neuer Wunsch/ }));

		await user.selectOptions(screen.getByLabelText('Serie'), '');

		expect(screen.queryByTestId('candidate-list')).not.toBeInTheDocument();
		expect(screen.getByRole('button', { name: 'Suchen' })).toBeDisabled();
		expect(mocks.fetchWantedCandidates).toHaveBeenCalledOnce();
	});

	it('adds an individual candidate and marks it as already wanted', async () => {
		render(WantedAddPage);
		const user = userEvent.setup();

		await waitFor(() => expect(screen.getByTestId('series-select')).toBeInTheDocument());
		await user.selectOptions(screen.getByLabelText('Serie'), 'maddrax');
		await waitFor(() =>
			expect(screen.getByRole('checkbox', { name: /Neuer Wunsch/ })).toBeEnabled()
		);
		await user.click(screen.getByRole('checkbox', { name: /Neuer Wunsch/ }));
		await user.click(screen.getByTestId('add-selection'));

		await waitFor(() => expect(mocks.addWantedBulk).toHaveBeenCalledWith([1]));
		expect(screen.getByRole('checkbox', { name: /Neuer Wunsch/ })).toBeDisabled();
		expect(screen.getByText(/1 Hefte zur Wunschliste hinzugefügt/)).toBeInTheDocument();
	});

	it('shows the saving state and handles an idempotent unchanged result', async () => {
		const pending = deferred<{
			created: never[];
			unchanged: Array<{ issue_id: number; entry_id: number }>;
			rejected: never[];
		}>();
		mocks.addWantedBulk.mockReturnValueOnce(pending.promise);
		render(WantedAddPage);
		const user = userEvent.setup();

		await waitFor(() => expect(screen.getByTestId('series-select')).toBeInTheDocument());
		await user.selectOptions(screen.getByLabelText('Serie'), 'maddrax');
		await waitFor(() =>
			expect(screen.getByRole('checkbox', { name: /Neuer Wunsch/ })).toBeEnabled()
		);
		await user.click(screen.getByRole('checkbox', { name: /Neuer Wunsch/ }));
		await user.click(screen.getByTestId('add-selection'));

		expect(screen.getByTestId('add-selection')).toHaveTextContent('Speichere …');
		expect(screen.getByTestId('add-selection')).toBeDisabled();
		pending.resolve({
			created: [],
			unchanged: [{ issue_id: 1, entry_id: 11 }],
			rejected: []
		});

		await waitFor(() =>
			expect(screen.getByRole('checkbox', { name: /Neuer Wunsch/ })).toBeDisabled()
		);
		expect(screen.getByText(/1 bereits vorhanden/)).toBeInTheDocument();
	});

	it('selects and deselects all available candidates on the current page', async () => {
		mocks.fetchWantedCandidates.mockResolvedValue({
			data: [newCandidate, { ...newCandidate, issue_id: 3, issue_number: 3, title: 'Dritter' }],
			page: 1,
			per_page: 50,
			total: 2
		});
		render(WantedAddPage);
		const user = userEvent.setup();

		await waitFor(() => expect(screen.getByTestId('series-select')).toBeInTheDocument());
		await user.selectOptions(screen.getByLabelText('Serie'), 'maddrax');
		await waitFor(() => expect(screen.getAllByRole('checkbox')).toHaveLength(2));
		await user.click(screen.getByTestId('toggle-all'));
		expect(screen.getByTestId('add-selection')).toHaveTextContent('2 ausgewählte hinzufügen');

		await user.click(screen.getByTestId('toggle-all'));
		expect(screen.getByTestId('add-selection')).toBeDisabled();
	});

	it('keeps bulk selection empty when all visible candidates are already wanted', async () => {
		mocks.fetchWantedCandidates.mockResolvedValue({
			data: [existingCandidate],
			page: 1,
			per_page: 50,
			total: 1
		});
		render(WantedAddPage);
		const user = userEvent.setup();

		await waitFor(() => expect(screen.getByTestId('series-select')).toBeInTheDocument());
		await user.selectOptions(screen.getByLabelText('Serie'), 'maddrax');
		await waitFor(() => expect(screen.getByTestId('toggle-all')).toBeInTheDocument());
		await user.click(screen.getByTestId('toggle-all'));

		expect(screen.getByTestId('add-selection')).toBeDisabled();
		expect(mocks.addWantedBulk).not.toHaveBeenCalled();
	});

	it('searches candidates and resets to the first page', async () => {
		render(WantedAddPage);
		const user = userEvent.setup();

		await waitFor(() => expect(screen.getByTestId('series-select')).toBeInTheDocument());
		await user.selectOptions(screen.getByLabelText('Serie'), 'maddrax');
		await waitFor(() => expect(screen.getByTestId('candidate-list')).toBeInTheDocument());
		await user.type(screen.getByLabelText('Titel oder Autor'), '  Zybell  ');
		await user.click(screen.getByRole('button', { name: 'Suchen' }));

		await waitFor(() =>
			expect(mocks.fetchWantedCandidates).toHaveBeenLastCalledWith({
				series_slug: 'maddrax',
				q: 'Zybell',
				page: 1,
				per_page: 50
			})
		);
	});

	it('loads the next and previous candidate pages', async () => {
		mocks.fetchWantedCandidates
			.mockResolvedValueOnce({ data: [newCandidate], page: 1, per_page: 50, total: 51 })
			.mockResolvedValueOnce({
				data: [{ ...newCandidate, issue_id: 51, issue_number: 51, title: 'Seite 2' }],
				page: 2,
				per_page: 50,
				total: 51
			})
			.mockResolvedValueOnce({ data: [newCandidate], page: 1, per_page: 50, total: 51 });
		render(WantedAddPage);
		const user = userEvent.setup();

		await waitFor(() => expect(screen.getByTestId('series-select')).toBeInTheDocument());
		await user.selectOptions(screen.getByLabelText('Serie'), 'maddrax');
		await waitFor(() => expect(screen.getByRole('button', { name: 'Weiter' })).toBeEnabled());
		await user.click(screen.getByRole('button', { name: 'Weiter' }));

		await waitFor(() => expect(screen.getByText('Seite 2')).toBeInTheDocument());
		expect(mocks.fetchWantedCandidates).toHaveBeenLastCalledWith({
			series_slug: 'maddrax',
			q: undefined,
			page: 2,
			per_page: 50
		});
		await user.click(screen.getByRole('button', { name: 'Zurück' }));

		await waitFor(() => expect(screen.getByText('Neuer Wunsch')).toBeInTheDocument());
		expect(mocks.fetchWantedCandidates).toHaveBeenLastCalledWith({
			series_slug: 'maddrax',
			q: undefined,
			page: 1,
			per_page: 50
		});
	});

	it('shows an empty result for a selected series', async () => {
		mocks.fetchWantedCandidates.mockResolvedValueOnce({
			data: [],
			page: 1,
			per_page: 50,
			total: 0
		});
		render(WantedAddPage);
		const user = userEvent.setup();

		await waitFor(() => expect(screen.getByTestId('series-select')).toBeInTheDocument());
		await user.selectOptions(screen.getByLabelText('Serie'), 'maddrax');

		await waitFor(() => expect(screen.getByTestId('candidates-empty')).toBeInTheDocument());
	});

	it.each([
		[new Error('Kandidatenfehler'), 'Kandidatenfehler'],
		['untyped failure', 'Fehlende Hefte konnten nicht geladen werden.']
	])('reports candidate loading failures from %s', async (cause, expectedMessage) => {
		mocks.fetchWantedCandidates.mockRejectedValueOnce(cause);
		render(WantedAddPage);
		const user = userEvent.setup();

		await waitFor(() => expect(screen.getByTestId('series-select')).toBeInTheDocument());
		await user.selectOptions(screen.getByLabelText('Serie'), 'maddrax');

		await waitFor(() => expect(screen.getByRole('alert')).toHaveTextContent(expectedMessage));
	});

	it('removes concurrently owned rejections from the candidate list', async () => {
		mocks.addWantedBulk.mockResolvedValue({
			created: [],
			unchanged: [],
			rejected: [{ issue_id: 1, reason: 'already_owned' }]
		});
		render(WantedAddPage);
		const user = userEvent.setup();

		await waitFor(() => expect(screen.getByTestId('series-select')).toBeInTheDocument());
		await user.selectOptions(screen.getByLabelText('Serie'), 'maddrax');
		await waitFor(() =>
			expect(screen.getByRole('checkbox', { name: /Neuer Wunsch/ })).toBeEnabled()
		);
		await user.click(screen.getByRole('checkbox', { name: /Neuer Wunsch/ }));
		await user.click(screen.getByTestId('add-selection'));

		await waitFor(() => expect(screen.queryByText('Neuer Wunsch')).not.toBeInTheDocument());
		expect(screen.getByText(/1 abgelehnt/)).toBeInTheDocument();
	});

	it('shows loading, empty and error states', async () => {
		mocks.fetchSeries.mockResolvedValueOnce([]);
		const empty = render(WantedAddPage);
		await waitFor(() => expect(screen.getByTestId('series-empty')).toBeInTheDocument());
		empty.unmount();

		mocks.fetchSeries.mockRejectedValueOnce(new Error('Serienfehler'));
		render(WantedAddPage);
		await waitFor(() => expect(screen.getByRole('alert')).toHaveTextContent('Serienfehler'));
	});

	it('uses the fallback message for an untyped series failure', async () => {
		mocks.fetchSeries.mockRejectedValueOnce('untyped failure');
		render(WantedAddPage);

		await waitFor(() =>
			expect(screen.getByRole('alert')).toHaveTextContent('Serien konnten nicht geladen werden.')
		);
	});

	it('keeps the selection when bulk persistence fails', async () => {
		mocks.addWantedBulk.mockRejectedValueOnce(new Error('Speicherfehler'));
		render(WantedAddPage);
		const user = userEvent.setup();

		await waitFor(() => expect(screen.getByTestId('series-select')).toBeInTheDocument());
		await user.selectOptions(screen.getByLabelText('Serie'), 'maddrax');
		await waitFor(() =>
			expect(screen.getByRole('checkbox', { name: /Neuer Wunsch/ })).toBeEnabled()
		);
		await user.click(screen.getByRole('checkbox', { name: /Neuer Wunsch/ }));
		await user.click(screen.getByTestId('add-selection'));

		await waitFor(() => expect(screen.getByRole('alert')).toHaveTextContent('Speicherfehler'));
		expect(screen.getByTestId('add-selection')).toHaveTextContent('1 ausgewählte hinzufügen');
	});

	it('uses the fallback message for an untyped bulk persistence failure', async () => {
		mocks.addWantedBulk.mockRejectedValueOnce('untyped failure');
		render(WantedAddPage);
		const user = userEvent.setup();

		await waitFor(() => expect(screen.getByTestId('series-select')).toBeInTheDocument());
		await user.selectOptions(screen.getByLabelText('Serie'), 'maddrax');
		await waitFor(() =>
			expect(screen.getByRole('checkbox', { name: /Neuer Wunsch/ })).toBeEnabled()
		);
		await user.click(screen.getByRole('checkbox', { name: /Neuer Wunsch/ }));
		await user.click(screen.getByTestId('add-selection'));

		await waitFor(() =>
			expect(screen.getByRole('alert')).toHaveTextContent(
				'Wünsche konnten nicht gespeichert werden.'
			)
		);
		expect(screen.getByTestId('add-selection')).toHaveTextContent('1 ausgewählte hinzufügen');
	});

	it('redirects unauthenticated users without loading series', async () => {
		const { goto } = await import('$app/navigation');
		mocks.getAuthState.mockReturnValue({ isAuthenticated: false, isLoading: false, user: null });

		render(WantedAddPage);

		await waitFor(() => expect(goto).toHaveBeenCalledWith('/login'));
		expect(mocks.fetchSeries).not.toHaveBeenCalled();
	});
});
