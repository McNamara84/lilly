import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/svelte';
import { userEvent } from '@testing-library/user-event';
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
const mockDeleteCollectionEntry = vi.fn();

vi.mock('$lib/api/collection', () => ({
	fetchAllCollectionEntries: (...args: unknown[]) => mockFetchAllCollectionEntries(...args),
	addToCollection: (...args: unknown[]) => mockAddToCollection(...args),
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
		issue_number: 2,
		title: 'Der Gott der Lava',
		cover_url: null,
		cover_local_path: null,
		source_wiki_url: null,
		authors: [],
		cycle: null
	}
];

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

describe('Collection Add Page', () => {
	beforeEach(() => {
		vi.clearAllMocks();
		mockFetchSeries.mockResolvedValue(mockSeries);
		mockFetchAllSeriesIssues.mockResolvedValue(mockIssues);
		mockFetchAllCollectionEntries.mockResolvedValue([]);
	});

	it('sets the page title', () => {
		mockGetAuthState.mockReturnValue(authedState());
		render(AddPage);

		expect(document.title).toContain('Hefte hinzufügen');
	});

	it('shows series selection initially', async () => {
		mockGetAuthState.mockReturnValue(authedState());
		render(AddPage);

		expect(screen.getByTestId('add-title')).toHaveTextContent('Serie wählen');

		await waitFor(() => {
			expect(screen.getByTestId('series-selector')).toBeInTheDocument();
		});

		expect(screen.getAllByTestId('series-card')).toHaveLength(2);
		expect(screen.getByText('Maddrax')).toBeInTheDocument();
		expect(screen.getByText('Perry Rhodan')).toBeInTheDocument();
	});

	it('shows series total issues count', async () => {
		mockGetAuthState.mockReturnValue(authedState());
		render(AddPage);

		await waitFor(() => {
			expect(screen.getByText('620 Hefte')).toBeInTheDocument();
		});
	});

	it('shows loading state', () => {
		mockGetAuthState.mockReturnValue(authedState());
		mockFetchSeries.mockReturnValue(new Promise(() => {}));
		render(AddPage);

		expect(screen.getByTestId('loading-indicator')).toHaveTextContent('Lade Serien...');
	});

	it('shows empty state when no series exist', async () => {
		mockGetAuthState.mockReturnValue(authedState());
		mockFetchSeries.mockResolvedValue([]);
		render(AddPage);

		await waitFor(() => {
			expect(screen.getByTestId('empty-state')).toHaveTextContent('Noch keine Serien verfügbar.');
		});
	});

	it('shows error message on series load failure', async () => {
		mockGetAuthState.mockReturnValue(authedState());
		mockFetchSeries.mockRejectedValue(new Error('Server error'));
		render(AddPage);

		await waitFor(() => {
			expect(screen.getByTestId('error-message')).toBeInTheDocument();
			expect(screen.getByText('Server error')).toBeInTheDocument();
		});
	});

	it('shows number grid after selecting a series', async () => {
		mockGetAuthState.mockReturnValue(authedState());
		render(AddPage);
		const user = userEvent.setup();

		await waitFor(() => {
			expect(screen.getByText('Maddrax')).toBeInTheDocument();
		});

		await user.click(screen.getAllByTestId('series-card')[0]);

		await waitFor(() => {
			expect(screen.getByTestId('number-grid')).toBeInTheDocument();
		});
		expect(screen.getAllByTestId('number-cell')).toHaveLength(2);
	});

	it('shows back button when series is selected', async () => {
		mockGetAuthState.mockReturnValue(authedState());
		render(AddPage);
		const user = userEvent.setup();

		await waitFor(() => {
			expect(screen.getByText('Maddrax')).toBeInTheDocument();
		});

		await user.click(screen.getAllByTestId('series-card')[0]);

		await waitFor(() => {
			expect(screen.getByTestId('back-button')).toBeInTheDocument();
		});
	});

	it('updates title to series name after selection', async () => {
		mockGetAuthState.mockReturnValue(authedState());
		render(AddPage);
		const user = userEvent.setup();

		await waitFor(() => {
			expect(screen.getByText('Maddrax')).toBeInTheDocument();
		});

		await user.click(screen.getAllByTestId('series-card')[0]);

		await waitFor(() => {
			expect(screen.getByTestId('add-title')).toHaveTextContent('Maddrax');
		});
	});

	it('redirects unauthenticated users to login', async () => {
		const { goto } = await import('$app/navigation');
		mockGetAuthState.mockReturnValue({
			isAuthenticated: false,
			user: null,
			isLoading: false
		});

		render(AddPage);

		await waitFor(() => {
			expect(goto).toHaveBeenCalledWith('/login');
		});
	});

	it('adds issue to collection when cell is clicked', async () => {
		mockGetAuthState.mockReturnValue(authedState());
		mockAddToCollection.mockResolvedValue({
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
			condition_grade: 'Z2',
			status: 'owned',
			notes: null,
			created_at: '2026-03-22T10:00:00Z',
			updated_at: '2026-03-22T10:00:00Z'
		});
		render(AddPage);
		const user = userEvent.setup();

		await waitFor(() => {
			expect(screen.getByText('Maddrax')).toBeInTheDocument();
		});
		await user.click(screen.getAllByTestId('series-card')[0]);

		await waitFor(() => {
			expect(screen.getByTestId('number-grid')).toBeInTheDocument();
		});

		await user.click(screen.getAllByTestId('number-cell')[0]);

		await waitFor(() => {
			expect(mockAddToCollection).toHaveBeenCalledWith({
				issue_id: 100,
				condition_grade: 'Z2',
				status: 'owned'
			});
		});
	});

	it('removes issue from collection when in-collection cell is clicked', async () => {
		mockGetAuthState.mockReturnValue(authedState());
		const existingEntry = {
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
			condition_grade: 'Z2',
			status: 'owned',
			notes: null,
			created_at: '2026-03-22T10:00:00Z',
			updated_at: '2026-03-22T10:00:00Z'
		};
		mockFetchAllCollectionEntries.mockResolvedValue([existingEntry]);
		mockDeleteCollectionEntry.mockResolvedValue(undefined);
		render(AddPage);
		const user = userEvent.setup();

		await waitFor(() => {
			expect(screen.getByText('Maddrax')).toBeInTheDocument();
		});
		await user.click(screen.getAllByTestId('series-card')[0]);

		await waitFor(() => {
			expect(screen.getByTestId('number-grid')).toBeInTheDocument();
		});

		// Click the cell that's already in the collection
		await user.click(screen.getAllByTestId('number-cell')[0]);

		await waitFor(() => {
			expect(mockDeleteCollectionEntry).toHaveBeenCalledWith(10);
		});
	});

	it('shows error toast when add fails', async () => {
		mockGetAuthState.mockReturnValue(authedState());
		mockAddToCollection.mockRejectedValue(new Error('Add failed'));
		render(AddPage);
		const user = userEvent.setup();

		await waitFor(() => {
			expect(screen.getByText('Maddrax')).toBeInTheDocument();
		});
		await user.click(screen.getAllByTestId('series-card')[0]);

		await waitFor(() => {
			expect(screen.getByTestId('number-grid')).toBeInTheDocument();
		});

		await user.click(screen.getAllByTestId('number-cell')[0]);

		await waitFor(() => {
			expect(screen.getByText('Add failed')).toBeInTheDocument();
		});
	});

	it('shows error toast when remove fails', async () => {
		mockGetAuthState.mockReturnValue(authedState());
		mockFetchAllCollectionEntries.mockResolvedValue([
			{
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
				condition_grade: 'Z2',
				status: 'owned',
				notes: null,
				created_at: '2026-03-22T10:00:00Z',
				updated_at: '2026-03-22T10:00:00Z'
			}
		]);
		mockDeleteCollectionEntry.mockRejectedValue(new Error('Server error'));
		render(AddPage);
		const user = userEvent.setup();

		await waitFor(() => {
			expect(screen.getByText('Maddrax')).toBeInTheDocument();
		});
		await user.click(screen.getAllByTestId('series-card')[0]);

		await waitFor(() => {
			expect(screen.getByTestId('number-grid')).toBeInTheDocument();
		});

		await user.click(screen.getAllByTestId('number-cell')[0]);

		await waitFor(() => {
			expect(screen.getByText('Fehler beim Entfernen')).toBeInTheDocument();
		});
	});

	it('returns to series selection when back button is clicked', async () => {
		mockGetAuthState.mockReturnValue(authedState());
		render(AddPage);
		const user = userEvent.setup();

		await waitFor(() => {
			expect(screen.getByText('Maddrax')).toBeInTheDocument();
		});
		await user.click(screen.getAllByTestId('series-card')[0]);

		await waitFor(() => {
			expect(screen.getByTestId('back-button')).toBeInTheDocument();
		});

		await user.click(screen.getByTestId('back-button'));

		await waitFor(() => {
			expect(screen.getByTestId('add-title')).toHaveTextContent('Serie wählen');
		});
		expect(screen.getByTestId('series-selector')).toBeInTheDocument();
	});

	it('shows error when issue loading fails', async () => {
		mockGetAuthState.mockReturnValue(authedState());
		mockFetchAllSeriesIssues.mockRejectedValue(new Error('Failed to load issues'));
		render(AddPage);
		const user = userEvent.setup();

		await waitFor(() => {
			expect(screen.getByText('Maddrax')).toBeInTheDocument();
		});
		await user.click(screen.getAllByTestId('series-card')[0]);

		await waitFor(() => {
			expect(screen.getByTestId('error-message')).toBeInTheDocument();
		});
	});

	it('shows empty state when series has no issues', async () => {
		mockGetAuthState.mockReturnValue(authedState());
		mockFetchAllSeriesIssues.mockResolvedValue([]);
		render(AddPage);
		const user = userEvent.setup();

		await waitFor(() => {
			expect(screen.getByText('Maddrax')).toBeInTheDocument();
		});
		await user.click(screen.getAllByTestId('series-card')[0]);

		await waitFor(() => {
			expect(screen.getByTestId('empty-state')).toHaveTextContent('Keine Hefte in dieser Serie.');
		});
	});
});
