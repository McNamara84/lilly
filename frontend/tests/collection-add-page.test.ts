import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/svelte';
import { userEvent } from '@testing-library/user-event';
import { tick } from 'svelte';
import AddPage from '../src/routes/collection/add/+page.svelte';

const mockGetAuthState = vi.fn();

vi.mock('$lib/stores/auth.svelte', () => ({
	getAuthState: () => mockGetAuthState()
}));

const mockFetchSeries = vi.fn();
const mockFetchAllSeriesIssues = vi.fn();

vi.mock('$lib/api/series', () => ({
	fetchSeries: () => mockFetchSeries(),
	fetchAllSeriesIssues: (...args: unknown[]) => mockFetchAllSeriesIssues(...args)
}));

const mockFetchAllCollectionEntries = vi.fn();
const mockAddToCollection = vi.fn();
const mockUpdateCollectionEntry = vi.fn();
const mockDeleteCollectionEntry = vi.fn();

vi.mock('$lib/api/collection', () => ({
	fetchAllCollectionEntries: (...args: unknown[]) => mockFetchAllCollectionEntries(...args),
	addToCollection: (...args: unknown[]) => mockAddToCollection(...args),
	updateCollectionEntry: (...args: unknown[]) => mockUpdateCollectionEntry(...args),
	deleteCollectionEntry: (...args: unknown[]) => mockDeleteCollectionEntry(...args)
}));

vi.mock('$app/navigation', () => ({
	goto: vi.fn()
}));

vi.mock('$app/paths', () => ({
	resolve: (path: string) => path
}));

const mockSeries = [
	{
		id: 1,
		name: 'Maddrax',
		slug: 'maddrax',
		publisher: 'Bastei',
		genre: 'Science-Fiction',
		frequency: 'biweekly',
		total_issues: 620,
		status: 'running',
		active: true,
		source_url: null
	},
	{
		id: 2,
		name: 'Perry Rhodan',
		slug: 'perry-rhodan',
		publisher: 'Pabel-Moewig',
		genre: 'Science-Fiction',
		frequency: 'weekly',
		total_issues: 3300,
		status: 'running',
		active: true,
		source_url: null
	}
];

const mockIssues = [
	{
		id: 100,
		series_id: 1,
		issue_number: 1,
		title: 'Dunkle Zukunft',
		cover_url: null,
		cover_local_path: null,
		source_wiki_url: null,
		authors: [],
		cycle: null
	},
	{
		id: 101,
		series_id: 1,
		issue_number: 2,
		title: 'Der Gott der Lava',
		cover_url: null,
		cover_local_path: null,
		source_wiki_url: null,
		authors: [],
		cycle: null
	}
];

function makeEntry(overrides: Record<string, unknown> = {}) {
	return {
		id: 10,
		issue_id: 100,
		issue_number: 1,
		title: 'Dunkle Zukunft',
		series_id: 1,
		series_name: 'Maddrax',
		series_slug: 'maddrax',
		cover_url: null,
		cover_local_path: null,
		copy_number: 1,
		edition_label: null,
		condition_grade: 'Z2',
		status: 'owned',
		notes: null,
		created_at: '2026-03-22T10:00:00Z',
		updated_at: '2026-03-22T10:00:00Z',
		...overrides
	};
}

function authedState() {
	return {
		isAuthenticated: true,
		user: {
			id: 1,
			email: 'test@test.com',
			display_name: 'Test',
			email_verified: true,
			role: 'user' as const
		},
		isLoading: false
	};
}

async function selectMaddrax(user: ReturnType<typeof userEvent.setup>) {
	await waitFor(() => expect(screen.getByText('Maddrax')).toBeInTheDocument());
	await user.click(screen.getAllByTestId('series-card')[0]);
	await waitFor(() => expect(screen.getByTestId('series-status-grid')).toBeInTheDocument());
}

describe('Collection Add Page', () => {
	beforeEach(() => {
		vi.restoreAllMocks();
		vi.clearAllMocks();
		mockGetAuthState.mockReturnValue(authedState());
		mockFetchSeries.mockResolvedValue(mockSeries);
		mockFetchAllSeriesIssues.mockResolvedValue(mockIssues);
		mockFetchAllCollectionEntries.mockResolvedValue([]);
	});

	it('sets the page title for the series grid', () => {
		render(AddPage);

		expect(document.title).toContain('Serienraster');
	});

	it('shows series selection with issue totals initially', async () => {
		render(AddPage);

		expect(screen.getByTestId('add-title')).toHaveTextContent('Serie wählen');
		await waitFor(() => expect(screen.getByTestId('series-selector')).toBeInTheDocument());
		expect(screen.getAllByTestId('series-card')).toHaveLength(2);
		expect(screen.getByText('Maddrax')).toBeInTheDocument();
		expect(screen.getByText('Perry Rhodan')).toBeInTheDocument();
		expect(screen.getByText('620 Hefte')).toBeInTheDocument();
	});

	it('omits an issue total when the series does not provide one', async () => {
		mockFetchSeries.mockResolvedValue([{ ...mockSeries[0], total_issues: 0 }]);

		render(AddPage);

		await waitFor(() => expect(screen.getByText('Maddrax')).toBeInTheDocument());
		expect(screen.queryByText('0 Hefte')).not.toBeInTheDocument();
	});

	it('shows loading, empty and error states for the series list', async () => {
		mockFetchSeries.mockReturnValueOnce(new Promise(() => {}));
		const loadingView = render(AddPage);
		expect(screen.getByTestId('loading-indicator')).toHaveTextContent('Lade Serien …');
		loadingView.unmount();

		mockFetchSeries.mockResolvedValueOnce([]);
		const emptyView = render(AddPage);
		await waitFor(() =>
			expect(screen.getByTestId('empty-state')).toHaveTextContent('Noch keine Serien verfügbar.')
		);
		emptyView.unmount();

		mockFetchSeries.mockRejectedValueOnce(new Error('Server error'));
		render(AddPage);
		await waitFor(() => expect(screen.getByRole('alert')).toHaveTextContent('Server error'));
	});

	it('uses the fallback message for an untyped series loading failure', async () => {
		mockFetchSeries.mockRejectedValue('untyped failure');

		render(AddPage);

		await waitFor(() =>
			expect(screen.getByRole('alert')).toHaveTextContent('Serien konnten nicht geladen werden.')
		);
	});

	it('loads issues and collection entries before showing the four-state grid', async () => {
		render(AddPage);
		const user = userEvent.setup();

		await selectMaddrax(user);

		expect(mockFetchAllSeriesIssues).toHaveBeenCalledWith('maddrax');
		expect(mockFetchAllCollectionEntries).toHaveBeenCalledWith('maddrax');
		expect(screen.getAllByTestId('series-status-cell')).toHaveLength(2);
		expect(screen.getByTestId('add-title')).toHaveTextContent('Maddrax');
		for (const status of ['owned', 'duplicate', 'wanted', 'missing']) {
			expect(screen.getByTestId(`legend-${status}`)).toBeInTheDocument();
		}
	});

	it('opens details for a missing issue without adding it immediately', async () => {
		render(AddPage);
		const user = userEvent.setup();
		await selectMaddrax(user);

		await user.click(screen.getAllByTestId('series-status-cell')[0]);

		expect(screen.getByTestId('issue-detail-sheet')).toBeInTheDocument();
		expect(screen.getByTestId('save-button')).toHaveTextContent('Hinzufügen');
		expect(mockAddToCollection).not.toHaveBeenCalled();
	});

	it('adds the configured issue and updates its grid status', async () => {
		mockAddToCollection.mockResolvedValue(makeEntry());
		render(AddPage);
		const user = userEvent.setup();
		await selectMaddrax(user);

		const firstCell = screen.getAllByTestId('series-status-cell')[0];
		expect(firstCell).toHaveAttribute('data-status', 'missing');
		await user.click(firstCell);
		await user.type(screen.getByTestId('notes-textarea'), 'Signierte Ausgabe');
		await user.click(screen.getByTestId('save-button'));

		await waitFor(() => {
			expect(mockAddToCollection).toHaveBeenCalledWith({
				issue_id: 100,
				condition_grade: 'Z2',
				status: 'owned',
				notes: 'Signierte Ausgabe',
				edition_label: ''
			});
		});
		expect(firstCell).toHaveAttribute('data-status', 'owned');
		expect(screen.getByTestId('toast')).toHaveTextContent('Heft #1 hinzugefügt');
	});

	it('edits an existing entry from the grid', async () => {
		const existingEntry = makeEntry({ notes: 'Alt' });
		const updatedEntry = makeEntry({ condition_grade: 'Z4', status: 'wanted', notes: 'Neu' });
		mockFetchAllCollectionEntries.mockResolvedValue([existingEntry]);
		mockUpdateCollectionEntry.mockResolvedValue(updatedEntry);
		render(AddPage);
		const user = userEvent.setup();
		await selectMaddrax(user);

		await user.click(screen.getAllByTestId('series-status-cell')[0]);
		expect(screen.getByTestId('save-button')).toHaveTextContent('Speichern');
		await user.click(screen.getByTestId('condition-chip-Z4'));
		await user.click(screen.getByTestId('status-wanted'));
		await user.clear(screen.getByTestId('notes-textarea'));
		await user.type(screen.getByTestId('notes-textarea'), 'Neu');
		await user.click(screen.getByTestId('save-button'));

		await waitFor(() =>
			expect(mockUpdateCollectionEntry).toHaveBeenCalledWith(10, {
				condition_grade: 'Z4',
				status: 'wanted',
				notes: 'Neu',
				edition_label: ''
			})
		);
		expect(screen.getAllByTestId('series-status-cell')[0]).toHaveAttribute('data-status', 'wanted');
	});

	it('keeps unrelated entries while updating and replaces an active toast', async () => {
		const firstEntry = makeEntry();
		const secondEntry = makeEntry({ id: 11, issue_id: 101, issue_number: 2 });
		const updatedEntry = makeEntry({ notes: 'Aktualisiert' });
		mockFetchAllCollectionEntries.mockResolvedValue([firstEntry, secondEntry]);
		mockUpdateCollectionEntry.mockResolvedValue(updatedEntry);
		const clearTimeoutSpy = vi.spyOn(globalThis, 'clearTimeout');
		const setTimeoutSpy = vi.spyOn(globalThis, 'setTimeout');
		render(AddPage);
		const user = userEvent.setup();
		await selectMaddrax(user);

		await user.click(screen.getAllByTestId('series-status-cell')[0]);
		await user.click(screen.getByTestId('save-button'));
		await waitFor(() => expect(screen.getByTestId('toast')).toHaveTextContent('aktualisiert'));

		await user.click(screen.getAllByTestId('series-status-cell')[0]);
		await user.click(screen.getByTestId('save-button'));
		await waitFor(() => expect(mockUpdateCollectionEntry).toHaveBeenCalledTimes(2));
		expect(clearTimeoutSpy).toHaveBeenCalled();
		expect(screen.getAllByTestId('series-status-cell')[1]).toHaveAttribute('data-status', 'owned');

		const toastTimer = setTimeoutSpy.mock.calls.find(([, delay]) => delay === 2500);
		expect(toastTimer).toBeDefined();
		(toastTimer?.[0] as () => void)();
		await tick();
		expect(screen.queryByTestId('toast')).not.toBeInTheDocument();
	});

	it('removes an existing entry only through the explicit delete action', async () => {
		mockFetchAllCollectionEntries.mockResolvedValue([makeEntry()]);
		mockDeleteCollectionEntry.mockResolvedValue(undefined);
		render(AddPage);
		const user = userEvent.setup();
		await selectMaddrax(user);

		const firstCell = screen.getAllByTestId('series-status-cell')[0];
		await user.click(firstCell);
		expect(mockDeleteCollectionEntry).not.toHaveBeenCalled();
		await user.click(screen.getByTestId('delete-button'));

		await waitFor(() => expect(mockDeleteCollectionEntry).toHaveBeenCalledWith(10));
		expect(firstCell).toHaveAttribute('data-status', 'missing');
		expect(screen.getByTestId('toast')).toHaveTextContent('Heft #1 entfernt');
	});

	it('keeps the detail sheet open and reports save failures', async () => {
		mockAddToCollection.mockRejectedValue(new Error('Add failed'));
		render(AddPage);
		const user = userEvent.setup();
		await selectMaddrax(user);

		await user.click(screen.getAllByTestId('series-status-cell')[0]);
		await user.click(screen.getByTestId('save-button'));

		await waitFor(() => expect(screen.getByTestId('sheet-error')).toHaveTextContent('Add failed'));
		expect(screen.getByTestId('issue-detail-sheet')).toBeInTheDocument();
	});

	it('uses the fallback message for an untyped save failure', async () => {
		mockAddToCollection.mockRejectedValue('untyped failure');
		render(AddPage);
		const user = userEvent.setup();
		await selectMaddrax(user);

		await user.click(screen.getAllByTestId('series-status-cell')[0]);
		await user.click(screen.getByTestId('save-button'));

		await waitFor(() =>
			expect(screen.getByTestId('sheet-error')).toHaveTextContent(
				'Eintrag konnte nicht gespeichert werden.'
			)
		);
	});

	it('keeps the entry and reports delete failures', async () => {
		mockFetchAllCollectionEntries.mockResolvedValue([makeEntry()]);
		mockDeleteCollectionEntry.mockRejectedValue(new Error('Delete failed'));
		render(AddPage);
		const user = userEvent.setup();
		await selectMaddrax(user);

		await user.click(screen.getAllByTestId('series-status-cell')[0]);
		await user.click(screen.getByTestId('delete-button'));

		await waitFor(() =>
			expect(screen.getByTestId('sheet-error')).toHaveTextContent('Delete failed')
		);
		expect(screen.getAllByTestId('series-status-cell')[0]).toHaveAttribute('data-status', 'owned');
	});

	it('uses the fallback message for an untyped delete failure', async () => {
		mockFetchAllCollectionEntries.mockResolvedValue([makeEntry()]);
		mockDeleteCollectionEntry.mockRejectedValue('untyped failure');
		render(AddPage);
		const user = userEvent.setup();
		await selectMaddrax(user);

		await user.click(screen.getAllByTestId('series-status-cell')[0]);
		await user.click(screen.getByTestId('delete-button'));

		await waitFor(() =>
			expect(screen.getByTestId('sheet-error')).toHaveTextContent(
				'Eintrag konnte nicht entfernt werden.'
			)
		);
	});

	it('restores focus to the selected grid cell after closing details', async () => {
		render(AddPage);
		const user = userEvent.setup();
		await selectMaddrax(user);

		const firstCell = screen.getAllByTestId('series-status-cell')[0];
		await user.click(firstCell);
		await user.click(screen.getByTestId('detail-sheet-backdrop'));

		await waitFor(() => expect(firstCell).toHaveFocus());
	});

	it('returns to the series selection', async () => {
		render(AddPage);
		const user = userEvent.setup();
		await selectMaddrax(user);

		await user.click(screen.getByTestId('back-button'));

		expect(screen.getByTestId('add-title')).toHaveTextContent('Serie wählen');
		expect(screen.getByTestId('series-selector')).toBeInTheDocument();
	});

	it('redirects unauthenticated users to login', async () => {
		const { goto } = await import('$app/navigation');
		mockGetAuthState.mockReturnValue({ isAuthenticated: false, user: null, isLoading: false });

		render(AddPage);

		await waitFor(() => expect(goto).toHaveBeenCalledWith('/login'));
	});

	it('waits for authentication to finish before loading or redirecting', async () => {
		const { goto } = await import('$app/navigation');
		mockGetAuthState.mockReturnValue({ isAuthenticated: false, user: null, isLoading: true });

		render(AddPage);

		expect(goto).not.toHaveBeenCalled();
		expect(mockFetchSeries).not.toHaveBeenCalled();
		expect(screen.getByTestId('loading-indicator')).toHaveTextContent('Lade Serien …');
	});

	it('shows issue loading errors and an empty series state', async () => {
		mockFetchAllSeriesIssues.mockRejectedValueOnce(new Error('Failed to load issues'));
		const errorView = render(AddPage);
		let user = userEvent.setup();
		await waitFor(() => expect(screen.getByText('Maddrax')).toBeInTheDocument());
		await user.click(screen.getAllByTestId('series-card')[0]);
		await waitFor(() =>
			expect(screen.getByTestId('error-message')).toHaveTextContent('Failed to load issues')
		);
		errorView.unmount();

		mockFetchAllSeriesIssues.mockResolvedValueOnce([]);
		render(AddPage);
		user = userEvent.setup();
		await waitFor(() => expect(screen.getByText('Maddrax')).toBeInTheDocument());
		await user.click(screen.getAllByTestId('series-card')[0]);
		await waitFor(() =>
			expect(screen.getByTestId('empty-state')).toHaveTextContent('Keine Hefte in dieser Serie.')
		);
	});

	it('uses the fallback message for an untyped issue loading failure', async () => {
		mockFetchAllSeriesIssues.mockRejectedValue('untyped failure');
		render(AddPage);
		const user = userEvent.setup();

		await waitFor(() => expect(screen.getByText('Maddrax')).toBeInTheDocument());
		await user.click(screen.getAllByTestId('series-card')[0]);

		await waitFor(() =>
			expect(screen.getByTestId('error-message')).toHaveTextContent(
				'Hefte konnten nicht geladen werden.'
			)
		);
	});
});
