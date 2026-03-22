import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/svelte';
import { userEvent } from '@testing-library/user-event';
import IssueDetailPage from '../src/routes/issues/[id]/+page.svelte';

const mockGetAuthState = vi.fn();

vi.mock('$lib/stores/auth.svelte', () => ({
	getAuthState: () => mockGetAuthState()
}));

const mockFetchIssue = vi.fn();

vi.mock('$lib/api/series', () => ({
	fetchIssue: (...args: unknown[]) => mockFetchIssue(...args)
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

function createMockStore<T>(initial: T) {
	let value = initial;
	const subs = new Set<(v: T) => void>();
	return {
		subscribe(fn: (v: T) => void) {
			subs.add(fn);
			fn(value);
			return () => subs.delete(fn);
		},
		set(v: T) {
			value = v;
			subs.forEach((fn) => fn(v));
		}
	};
}

const mockPage = createMockStore({ params: { id: '42' } });

vi.mock('$app/stores', () => ({
	page: { subscribe: (fn: (value: unknown) => void) => mockPage.subscribe(fn) }
}));

vi.mock('$app/navigation', () => ({
	goto: vi.fn()
}));

vi.mock('$app/paths', () => ({
	resolve: (path: string) => path
}));

const sampleIssue = {
	id: 42,
	series_id: 1,
	issue_number: 42,
	title: 'Dunkle Zukunft',
	cover_url: 'https://example.com/cover.jpg',
	cover_local_path: null,
	source_wiki_url: 'https://wiki.example.com/42',
	authors: ['Jo Zybell'],
	cycle: 'Kreuzfahrt',
	published_at: '2000-02-11',
	keywords: ['Zukunft', 'Mars'],
	cover_artists: [],
	notes: []
};

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
	condition_grade: 'Z1',
	status: 'owned',
	notes: 'Test note',
	created_at: '2026-03-22T10:00:00Z',
	updated_at: '2026-03-22T10:00:00Z'
};

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

function unauthState() {
	return {
		isAuthenticated: false,
		user: null,
		isLoading: false
	};
}

describe('Issue Detail Page', () => {
	beforeEach(() => {
		vi.clearAllMocks();
		mockFetchCollection.mockResolvedValue({ data: [], page: 1, per_page: 100, total: 0 });
	});

	it('shows loading state initially', () => {
		mockGetAuthState.mockReturnValue(authedState());
		mockFetchIssue.mockReturnValue(new Promise(() => {}));
		render(IssueDetailPage);

		expect(screen.getByTestId('loading-indicator')).toHaveTextContent('Lade Heft...');
	});

	it('renders issue details after loading', async () => {
		mockGetAuthState.mockReturnValue(authedState());
		mockFetchIssue.mockResolvedValue(sampleIssue);
		render(IssueDetailPage);

		await waitFor(() => {
			expect(screen.getByTestId('issue-detail-header')).toBeInTheDocument();
		});
		expect(screen.getByRole('heading', { level: 1, name: 'Dunkle Zukunft' })).toBeInTheDocument();
		expect(screen.getByText(/Heft #42/)).toBeInTheDocument();
		expect(screen.getByText('Jo Zybell')).toBeInTheDocument();
		expect(screen.getByText(/Kreuzfahrt/)).toBeInTheDocument();
	});

	it('renders cover image when available', async () => {
		mockGetAuthState.mockReturnValue(authedState());
		mockFetchIssue.mockResolvedValue(sampleIssue);
		render(IssueDetailPage);

		await waitFor(() => {
			expect(screen.getByTestId('issue-detail-cover')).toBeInTheDocument();
		});
		const img = screen.getByTestId('issue-detail-cover') as HTMLImageElement;
		expect(img.tagName).toBe('IMG');
		expect(img.src).toContain('cover.jpg');
	});

	it('shows placeholder when no cover', async () => {
		mockGetAuthState.mockReturnValue(authedState());
		mockFetchIssue.mockResolvedValue({ ...sampleIssue, cover_url: null });
		render(IssueDetailPage);

		await waitFor(() => {
			expect(screen.getByTestId('issue-detail-cover')).toBeInTheDocument();
		});
		const cover = screen.getByTestId('issue-detail-cover');
		expect(cover.tagName).toBe('DIV');
	});

	it('renders keywords', async () => {
		mockGetAuthState.mockReturnValue(authedState());
		mockFetchIssue.mockResolvedValue(sampleIssue);
		render(IssueDetailPage);

		await waitFor(() => {
			expect(screen.getByText('Zukunft')).toBeInTheDocument();
		});
		expect(screen.getByText('Mars')).toBeInTheDocument();
	});

	it('shows wiki link when available', async () => {
		mockGetAuthState.mockReturnValue(authedState());
		mockFetchIssue.mockResolvedValue(sampleIssue);
		render(IssueDetailPage);

		await waitFor(() => {
			expect(screen.getByTestId('issue-detail-wiki-link')).toBeInTheDocument();
		});
		expect(screen.getByTestId('issue-detail-wiki-link')).toHaveAttribute(
			'href',
			'https://wiki.example.com/42'
		);
	});

	it('hides wiki link when not available', async () => {
		mockGetAuthState.mockReturnValue(authedState());
		mockFetchIssue.mockResolvedValue({ ...sampleIssue, source_wiki_url: null });
		render(IssueDetailPage);

		await waitFor(() => {
			expect(screen.getByTestId('issue-detail-header')).toBeInTheDocument();
		});
		expect(screen.queryByTestId('issue-detail-wiki-link')).not.toBeInTheDocument();
	});

	it('shows error on fetch failure', async () => {
		mockGetAuthState.mockReturnValue(authedState());
		mockFetchIssue.mockRejectedValue(new Error('Not found'));
		render(IssueDetailPage);

		await waitFor(() => {
			expect(screen.getByTestId('error-message')).toBeInTheDocument();
		});
		expect(screen.getByText('Not found')).toBeInTheDocument();
	});

	it('shows error for invalid ID', async () => {
		mockGetAuthState.mockReturnValue(authedState());
		mockPage.set({ params: { id: 'abc' } });
		render(IssueDetailPage);

		await waitFor(() => {
			expect(screen.getByTestId('error-message')).toBeInTheDocument();
		});
		expect(screen.getByText('Ungültige Heft-ID')).toBeInTheDocument();
		mockPage.set({ params: { id: '42' } });
	});

	it('shows collection panel for authenticated users', async () => {
		mockGetAuthState.mockReturnValue(authedState());
		mockFetchIssue.mockResolvedValue(sampleIssue);
		render(IssueDetailPage);

		await waitFor(() => {
			expect(screen.getByTestId('issue-detail-collection-panel')).toBeInTheDocument();
		});
	});

	it('hides collection panel for unauthenticated users', async () => {
		mockGetAuthState.mockReturnValue(unauthState());
		mockFetchIssue.mockResolvedValue(sampleIssue);
		render(IssueDetailPage);

		await waitFor(() => {
			expect(screen.getByTestId('issue-detail-header')).toBeInTheDocument();
		});
		expect(screen.queryByTestId('issue-detail-collection-panel')).not.toBeInTheDocument();
	});

	it('shows "In deiner Sammlung" when entry exists', async () => {
		mockGetAuthState.mockReturnValue(authedState());
		mockFetchIssue.mockResolvedValue(sampleIssue);
		mockFetchCollection.mockResolvedValue({
			data: [sampleEntry],
			page: 1,
			per_page: 100,
			total: 1
		});
		render(IssueDetailPage);

		await waitFor(() => {
			expect(screen.getByText('In deiner Sammlung')).toBeInTheDocument();
		});
		expect(screen.getByRole('button', { name: /Speichern/ })).toBeInTheDocument();
		expect(screen.getByRole('button', { name: 'Entfernen' })).toBeInTheDocument();
	});

	it('calls addToCollection when add button is clicked', async () => {
		mockGetAuthState.mockReturnValue(authedState());
		mockFetchIssue.mockResolvedValue(sampleIssue);
		mockAddToCollection.mockResolvedValue(sampleEntry);
		render(IssueDetailPage);
		const user = userEvent.setup();

		await waitFor(() => {
			expect(screen.getByRole('button', { name: /Zur Sammlung hinzufügen/ })).toBeInTheDocument();
		});

		await user.click(screen.getByRole('button', { name: /Zur Sammlung hinzufügen/ }));

		await waitFor(() => {
			expect(mockAddToCollection).toHaveBeenCalledOnce();
		});
	});

	it('calls updateCollectionEntry when save button is clicked', async () => {
		mockGetAuthState.mockReturnValue(authedState());
		mockFetchIssue.mockResolvedValue(sampleIssue);
		mockFetchCollection.mockResolvedValue({
			data: [sampleEntry],
			page: 1,
			per_page: 100,
			total: 1
		});
		mockUpdateCollectionEntry.mockResolvedValue(sampleEntry);
		render(IssueDetailPage);
		const user = userEvent.setup();

		await waitFor(() => {
			expect(screen.getByRole('button', { name: /Speichern/ })).toBeInTheDocument();
		});

		await user.click(screen.getByRole('button', { name: /Speichern/ }));

		await waitFor(() => {
			expect(mockUpdateCollectionEntry).toHaveBeenCalledOnce();
		});
	});

	it('calls deleteCollectionEntry when remove button is clicked', async () => {
		mockGetAuthState.mockReturnValue(authedState());
		mockFetchIssue.mockResolvedValue(sampleIssue);
		mockFetchCollection.mockResolvedValue({
			data: [sampleEntry],
			page: 1,
			per_page: 100,
			total: 1
		});
		mockDeleteCollectionEntry.mockResolvedValue(undefined);
		render(IssueDetailPage);
		const user = userEvent.setup();

		await waitFor(() => {
			expect(screen.getByRole('button', { name: 'Entfernen' })).toBeInTheDocument();
		});

		await user.click(screen.getByRole('button', { name: 'Entfernen' }));

		await waitFor(() => {
			expect(mockDeleteCollectionEntry).toHaveBeenCalledOnce();
		});
	});

	it('shows error when add fails', async () => {
		mockGetAuthState.mockReturnValue(authedState());
		mockFetchIssue.mockResolvedValue(sampleIssue);
		mockAddToCollection.mockRejectedValue(new Error('Duplicate entry'));
		render(IssueDetailPage);
		const user = userEvent.setup();

		await waitFor(() => {
			expect(screen.getByRole('button', { name: /Zur Sammlung hinzufügen/ })).toBeInTheDocument();
		});

		await user.click(screen.getByRole('button', { name: /Zur Sammlung hinzufügen/ }));

		await waitFor(() => {
			expect(screen.getByText('Duplicate entry')).toBeInTheDocument();
		});
	});

	it('shows error when update fails', async () => {
		mockGetAuthState.mockReturnValue(authedState());
		mockFetchIssue.mockResolvedValue(sampleIssue);
		mockFetchCollection.mockResolvedValue({
			data: [sampleEntry],
			page: 1,
			per_page: 100,
			total: 1
		});
		mockUpdateCollectionEntry.mockRejectedValue(new Error('Update failed'));
		render(IssueDetailPage);
		const user = userEvent.setup();

		await waitFor(() => {
			expect(screen.getByRole('button', { name: /Speichern/ })).toBeInTheDocument();
		});

		await user.click(screen.getByRole('button', { name: /Speichern/ }));

		await waitFor(() => {
			expect(screen.getByText('Update failed')).toBeInTheDocument();
		});
	});

	it('shows error when delete fails', async () => {
		mockGetAuthState.mockReturnValue(authedState());
		mockFetchIssue.mockResolvedValue(sampleIssue);
		mockFetchCollection.mockResolvedValue({
			data: [sampleEntry],
			page: 1,
			per_page: 100,
			total: 1
		});
		mockDeleteCollectionEntry.mockRejectedValue(new Error('Delete failed'));
		render(IssueDetailPage);
		const user = userEvent.setup();

		await waitFor(() => {
			expect(screen.getByRole('button', { name: 'Entfernen' })).toBeInTheDocument();
		});

		await user.click(screen.getByRole('button', { name: 'Entfernen' }));

		await waitFor(() => {
			expect(screen.getByText('Delete failed')).toBeInTheDocument();
		});
	});

	it('renders issue without optional fields', async () => {
		mockGetAuthState.mockReturnValue(authedState());
		mockFetchIssue.mockResolvedValue({
			...sampleIssue,
			authors: [],
			cycle: null,
			published_at: null,
			keywords: [],
			source_wiki_url: null
		});
		render(IssueDetailPage);

		await waitFor(() => {
			expect(screen.getByRole('heading', { level: 1, name: 'Dunkle Zukunft' })).toBeInTheDocument();
		});
		expect(screen.queryByText('Jo Zybell')).not.toBeInTheDocument();
		expect(screen.queryByText(/Zyklus/)).not.toBeInTheDocument();
	});

	it('renders trade placeholder section', async () => {
		mockGetAuthState.mockReturnValue(authedState());
		mockFetchIssue.mockResolvedValue(sampleIssue);
		render(IssueDetailPage);

		await waitFor(() => {
			expect(screen.getByText('Tausch-Verfügbarkeit')).toBeInTheDocument();
		});
	});

	it('renders status radio buttons in collection panel', async () => {
		mockGetAuthState.mockReturnValue(authedState());
		mockFetchIssue.mockResolvedValue(sampleIssue);
		render(IssueDetailPage);

		await waitFor(() => {
			expect(screen.getByTestId('issue-detail-collection-panel')).toBeInTheDocument();
		});
		expect(screen.getByRole('radio', { name: 'Vorhanden' })).toBeInTheDocument();
		expect(screen.getByRole('radio', { name: 'Doppelt' })).toBeInTheDocument();
		expect(screen.getByRole('radio', { name: 'Gesucht' })).toBeInTheDocument();
	});

	it('shows notes textarea in collection panel', async () => {
		mockGetAuthState.mockReturnValue(authedState());
		mockFetchIssue.mockResolvedValue(sampleIssue);
		render(IssueDetailPage);

		await waitFor(() => {
			expect(screen.getByTestId('issue-detail-collection-panel')).toBeInTheDocument();
		});
		expect(screen.getByLabelText('Notizen')).toBeInTheDocument();
	});

	it('pre-fills condition grade from existing entry', async () => {
		mockGetAuthState.mockReturnValue(authedState());
		mockFetchIssue.mockResolvedValue(sampleIssue);
		mockFetchCollection.mockResolvedValue({
			data: [{ ...sampleEntry, condition_grade: 'Z3', notes: 'My notes' }],
			page: 1,
			per_page: 100,
			total: 1
		});
		render(IssueDetailPage);

		await waitFor(() => {
			expect(screen.getByText('In deiner Sammlung')).toBeInTheDocument();
		});
		const notesTextarea = screen.getByLabelText('Notizen') as HTMLTextAreaElement;
		expect(notesTextarea.value).toBe('My notes');
	});

	it('resets fields after successful delete', async () => {
		mockGetAuthState.mockReturnValue(authedState());
		mockFetchIssue.mockResolvedValue(sampleIssue);
		mockFetchCollection.mockResolvedValue({
			data: [sampleEntry],
			page: 1,
			per_page: 100,
			total: 1
		});
		mockDeleteCollectionEntry.mockResolvedValue(undefined);
		render(IssueDetailPage);
		const user = userEvent.setup();

		await waitFor(() => {
			expect(screen.getByRole('button', { name: 'Entfernen' })).toBeInTheDocument();
		});

		await user.click(screen.getByRole('button', { name: 'Entfernen' }));

		await waitFor(() => {
			// After delete, heading should switch back to "Zur Sammlung hinzufügen"
			expect(screen.getByRole('heading', { name: 'Zur Sammlung hinzufügen' })).toBeInTheDocument();
		});
	});

	it('prefers cover_local_path over cover_url', async () => {
		mockGetAuthState.mockReturnValue(authedState());
		mockFetchIssue.mockResolvedValue({
			...sampleIssue,
			cover_local_path: '/covers/local.jpg',
			cover_url: 'https://example.com/remote.jpg'
		});
		render(IssueDetailPage);

		await waitFor(() => {
			const img = screen.getByTestId('issue-detail-cover') as HTMLImageElement;
			expect(img.src).toContain('local.jpg');
		});
	});
});
