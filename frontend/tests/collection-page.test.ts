import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/svelte';
import CollectionPage from '../src/routes/collection/+page.svelte';

const mockGetAuthState = vi.fn();

vi.mock('$lib/stores/auth.svelte', () => ({
	getAuthState: () => mockGetAuthState()
}));

const mockFetchCollection = vi.fn();
const mockAddToCollection = vi.fn();
const mockUpdateCollectionEntry = vi.fn();
const mockDeleteCollectionEntry = vi.fn();

vi.mock('$lib/api/collection', () => ({
	fetchCollection: (...args: unknown[]) => mockFetchCollection(...args),
	addToCollection: (...args: unknown[]) => mockAddToCollection(...args),
	updateCollectionEntry: (...args: unknown[]) => mockUpdateCollectionEntry(...args),
	deleteCollectionEntry: (...args: unknown[]) => mockDeleteCollectionEntry(...args)
}));

const mockFetchSeries = vi.fn();
const mockFetchIssue = vi.fn();

vi.mock('$lib/api/series', () => ({
	fetchSeries: () => mockFetchSeries(),
	fetchIssue: (...args: unknown[]) => mockFetchIssue(...args)
}));

vi.mock('$app/navigation', () => ({
	goto: vi.fn()
}));

vi.mock('$app/paths', () => ({
	resolve: (path: string) => path
}));

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

const sampleEntry = {
	id: 1,
	issue_id: 42,
	issue_number: 42,
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

const sampleIssue = {
	id: 42,
	series_id: 1,
	issue_number: 42,
	title: 'Dunkle Zukunft',
	cover_url: null,
	cover_local_path: null,
	source_wiki_url: null,
	authors: [],
	cycle: null,
	published_at: null,
	keywords: [],
	cover_artists: [],
	notes: []
};

describe('Collection Page', () => {
	beforeEach(() => {
		vi.clearAllMocks();
		mockFetchSeries.mockResolvedValue([]);
		mockFetchCollection.mockResolvedValue({ data: [], page: 1, per_page: 20, total: 0 });
	});

	it('sets the page title', () => {
		mockGetAuthState.mockReturnValue(authedState());
		render(CollectionPage);

		expect(document.title).toContain('Sammlung');
	});

	it('renders the collection heading', async () => {
		mockGetAuthState.mockReturnValue(authedState());
		render(CollectionPage);

		expect(screen.getByTestId('collection-title')).toHaveTextContent('Meine Sammlung');
	});

	it('shows the add button that links to /collection/add', () => {
		mockGetAuthState.mockReturnValue(authedState());
		render(CollectionPage);

		const fab = screen.getByTestId('collection-fab');
		expect(fab).toHaveAttribute('href', '/collection/add');
		expect(fab).toHaveTextContent('Hinzufügen');
	});

	it('shows loading state initially', () => {
		mockGetAuthState.mockReturnValue(authedState());
		mockFetchCollection.mockReturnValue(new Promise(() => {}));
		render(CollectionPage);

		expect(screen.getByTestId('cover-grid-skeleton')).toBeInTheDocument();
	});

	it('shows empty grid when collection is empty', async () => {
		mockGetAuthState.mockReturnValue(authedState());
		render(CollectionPage);

		await waitFor(() => {
			expect(screen.getByTestId('cover-grid-empty')).toBeInTheDocument();
		});
	});

	it('renders collection entries after loading', async () => {
		mockGetAuthState.mockReturnValue(authedState());
		mockFetchCollection.mockResolvedValue({
			data: [sampleEntry, { ...sampleEntry, id: 2, issue_number: 43, title: 'Zweites Heft' }],
			page: 1,
			per_page: 20,
			total: 2
		});
		render(CollectionPage);

		await waitFor(() => {
			expect(screen.getAllByTestId('cover-card')).toHaveLength(2);
		});
	});

	it('shows total count after loading', async () => {
		mockGetAuthState.mockReturnValue(authedState());
		mockFetchCollection.mockResolvedValue({
			data: [sampleEntry],
			page: 1,
			per_page: 20,
			total: 1
		});
		render(CollectionPage);

		await waitFor(() => {
			expect(screen.getByText(/1 von 1 Einträgen/)).toBeInTheDocument();
		});
	});

	it('shows error message on fetch failure', async () => {
		mockGetAuthState.mockReturnValue(authedState());
		mockFetchCollection.mockRejectedValue(new Error('Network error'));
		render(CollectionPage);

		await waitFor(() => {
			expect(screen.getByTestId('collection-error')).toBeInTheDocument();
			expect(screen.getByText('Network error')).toBeInTheDocument();
		});
	});

	it('redirects unauthenticated users to login', async () => {
		const { goto } = await import('$app/navigation');
		mockGetAuthState.mockReturnValue({
			isAuthenticated: false,
			user: null,
			isLoading: false
		});

		render(CollectionPage);

		await waitFor(() => {
			expect(goto).toHaveBeenCalledWith('/login');
		});
	});

	it('opens detail sheet when a card is selected', async () => {
		mockGetAuthState.mockReturnValue(authedState());
		mockFetchCollection.mockResolvedValue({
			data: [sampleEntry],
			page: 1,
			per_page: 20,
			total: 1
		});
		mockFetchIssue.mockResolvedValue(sampleIssue);
		render(CollectionPage);

		await waitFor(() => {
			expect(screen.getAllByTestId('cover-card')).toHaveLength(1);
		});

		// Click the cover card to trigger handleSelect
		const card = screen.getByTestId('cover-card');
		await card.click();

		await waitFor(() => {
			expect(mockFetchIssue).toHaveBeenCalledWith(42);
		});

		// The detail sheet should appear
		await waitFor(() => {
			expect(screen.getByTestId('issue-detail-sheet')).toBeInTheDocument();
		});
	});

	it('calls updateCollectionEntry on save from detail sheet', async () => {
		mockGetAuthState.mockReturnValue(authedState());
		mockFetchCollection.mockResolvedValue({
			data: [sampleEntry],
			page: 1,
			per_page: 20,
			total: 1
		});
		mockFetchIssue.mockResolvedValue(sampleIssue);
		mockUpdateCollectionEntry.mockResolvedValue(sampleEntry);
		render(CollectionPage);

		await waitFor(() => {
			expect(screen.getByTestId('cover-card')).toBeInTheDocument();
		});

		await screen.getByTestId('cover-card').click();

		await waitFor(() => {
			expect(screen.getByTestId('issue-detail-sheet')).toBeInTheDocument();
		});

		// Click save button in the sheet
		await screen.getByTestId('save-button').click();

		await waitFor(() => {
			expect(mockUpdateCollectionEntry).toHaveBeenCalledOnce();
		});
	});

	it('calls addToCollection on save when entry is missing', async () => {
		const missingEntry = {
			...sampleEntry,
			id: 0,
			status: 'missing',
			copy_number: null,
			condition_grade: null,
			created_at: null,
			updated_at: null
		};
		mockGetAuthState.mockReturnValue(authedState());
		mockFetchCollection.mockResolvedValue({
			data: [missingEntry],
			page: 1,
			per_page: 20,
			total: 1
		});
		mockFetchIssue.mockResolvedValue(sampleIssue);
		mockAddToCollection.mockResolvedValue(sampleEntry);
		render(CollectionPage);

		await waitFor(() => {
			expect(screen.getByTestId('cover-card')).toBeInTheDocument();
		});

		await screen.getByTestId('cover-card').click();

		await waitFor(() => {
			expect(screen.getByTestId('issue-detail-sheet')).toBeInTheDocument();
		});

		await screen.getByTestId('save-button').click();

		await waitFor(() => {
			expect(mockAddToCollection).toHaveBeenCalledOnce();
			expect(mockUpdateCollectionEntry).not.toHaveBeenCalled();
		});
	});

	it('calls deleteCollectionEntry on delete from detail sheet', async () => {
		mockGetAuthState.mockReturnValue(authedState());
		mockFetchCollection.mockResolvedValue({
			data: [sampleEntry],
			page: 1,
			per_page: 20,
			total: 1
		});
		mockFetchIssue.mockResolvedValue(sampleIssue);
		mockDeleteCollectionEntry.mockResolvedValue(undefined);
		render(CollectionPage);

		await waitFor(() => {
			expect(screen.getByTestId('cover-card')).toBeInTheDocument();
		});

		await screen.getByTestId('cover-card').click();

		await waitFor(() => {
			expect(screen.getByTestId('issue-detail-sheet')).toBeInTheDocument();
		});

		// Click delete button in the sheet
		await screen.getByTestId('delete-button').click();

		await waitFor(() => {
			expect(mockDeleteCollectionEntry).toHaveBeenCalledWith(1);
		});
	});

	it('closes detail sheet when backdrop is clicked', async () => {
		mockGetAuthState.mockReturnValue(authedState());
		mockFetchCollection.mockResolvedValue({
			data: [sampleEntry],
			page: 1,
			per_page: 20,
			total: 1
		});
		mockFetchIssue.mockResolvedValue(sampleIssue);
		render(CollectionPage);

		await waitFor(() => {
			expect(screen.getByTestId('cover-card')).toBeInTheDocument();
		});

		await screen.getByTestId('cover-card').click();

		await waitFor(() => {
			expect(screen.getByTestId('detail-sheet-backdrop')).toBeInTheDocument();
		});

		await screen.getByTestId('detail-sheet-backdrop').click();

		await waitFor(() => {
			expect(screen.queryByTestId('issue-detail-sheet')).not.toBeInTheDocument();
		});
	});

	it('renders filter bar', async () => {
		mockGetAuthState.mockReturnValue(authedState());
		render(CollectionPage);

		await waitFor(() => {
			expect(screen.getByTestId('collection-filter-bar')).toBeInTheDocument();
		});
	});
});
