import { beforeEach, describe, expect, it, vi } from 'vitest';
import { render, screen, waitFor } from '@testing-library/svelte';
import PublicProfilePage from '../src/routes/users/[id]/+page.svelte';
import PublicCollectionPage from '../src/routes/users/[id]/collection/+page.svelte';

const mocks = vi.hoisted(() => ({
	page: { params: { id: '7' } },
	fetchPublicProfile: vi.fn(),
	fetchPublicCollection: vi.fn(),
	fetchPublicCollectionStats: vi.fn()
}));

vi.mock('$app/state', () => ({ page: mocks.page }));
vi.mock('$app/paths', () => ({ resolve: (path: string) => path }));
vi.mock('$lib/api/profile', () => ({
	fetchPublicProfile: (...args: unknown[]) => mocks.fetchPublicProfile(...args),
	fetchPublicCollection: (...args: unknown[]) => mocks.fetchPublicCollection(...args),
	fetchPublicCollectionStats: (...args: unknown[]) => mocks.fetchPublicCollectionStats(...args)
}));

const publicProfile = {
	id: 7,
	display_name: 'Sammler',
	avatar_url: null,
	location: 'Berlin',
	created_at: '2026-01-15T00:00:00'
};

const stats = {
	total_issues: 10,
	total_physical_owned: 3,
	total_owned: 2,
	total_duplicate: 1,
	total_wanted: 3,
	overall_progress_percent: 20,
	series_stats: [
		{
			series_id: 1,
			series_name: 'Maddrax',
			series_slug: 'maddrax',
			total_in_series: 10,
			owned_count: 2,
			duplicate_count: 1,
			wanted_count: 3,
			progress_percent: 20
		}
	]
};

function collectionEntry(overrides: Record<string, unknown> = {}) {
	return {
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
		notes: 'Erste Zeile\nGrüße 📚',
		...overrides
	};
}

describe('Public Profile Page', () => {
	beforeEach(() => {
		vi.clearAllMocks();
		mocks.page.params.id = '7';
		mocks.fetchPublicProfile.mockResolvedValue(publicProfile);
		mocks.fetchPublicCollectionStats.mockResolvedValue(stats);
	});

	it('shows only public identity data and links a public collection', async () => {
		render(PublicProfilePage);

		await waitFor(() => expect(screen.getByTestId('public-profile')).toBeInTheDocument());
		expect(screen.getByRole('heading', { level: 1 })).toHaveTextContent('Sammler');
		expect(screen.getByText('Berlin')).toBeInTheDocument();
		expect(screen.queryByText(/@example\.com/)).not.toBeInTheDocument();
		expect(screen.getByRole('link', { name: 'Sammlung öffnen' })).toHaveAttribute(
			'href',
			'/users/7/collection'
		);
		expect(screen.getByText(/2 von 10/)).toBeInTheDocument();
		expect(screen.getByTestId('public-physical-total')).toHaveTextContent('3 physische Hefte');
		expect(screen.getByTestId('public-physical-total')).toHaveTextContent(
			'2 unterschiedliche Ausgaben'
		);
	});

	it('renders a controlled avatar URL and falls back to initials', async () => {
		mocks.fetchPublicProfile.mockResolvedValue({
			...publicProfile,
			avatar_url: '/api/v1/users/7/avatar'
		});
		const withAvatar = render(PublicProfilePage);

		expect(await screen.findByAltText('Avatar von Sammler')).toHaveAttribute(
			'src',
			'/api/v1/users/7/avatar'
		);
		withAvatar.unmount();

		mocks.fetchPublicProfile.mockResolvedValue({
			...publicProfile,
			display_name: 'Mira Muster'
		});
		render(PublicProfilePage);
		expect(await screen.findByTestId('public-profile-avatar')).toHaveTextContent('MM');
	});

	it('uses singular labels for one physical and one distinct issue', async () => {
		mocks.fetchPublicCollectionStats.mockResolvedValue({
			...stats,
			total_physical_owned: 1,
			total_owned: 1
		});

		render(PublicProfilePage);

		const total = await screen.findByTestId('public-physical-total');
		expect(total).toHaveTextContent('1 physisches Heft');
		expect(total).toHaveTextContent('1 unterschiedliche Ausgabe');
	});

	it('shows an unknown series total without a misleading progress bar', async () => {
		mocks.fetchPublicCollectionStats.mockResolvedValue({
			...stats,
			total_issues: null,
			overall_progress_percent: null,
			series_stats: [
				{
					...stats.series_stats[0],
					total_in_series: null,
					progress_percent: null
				}
			]
		});

		render(PublicProfilePage);

		expect(await screen.findByText(/2 gesammelt — Gesamtzahl unbekannt/)).toBeVisible();
		expect(screen.getByTestId('progress-unavailable')).toBeInTheDocument();
		expect(screen.queryByRole('progressbar')).not.toBeInTheDocument();
	});

	it('renders a public profile without an optional location', async () => {
		mocks.fetchPublicProfile.mockResolvedValue({ ...publicProfile, location: null });

		render(PublicProfilePage);

		await waitFor(() => expect(screen.getByTestId('public-profile')).toBeInTheDocument());
		expect(screen.queryByText('Berlin')).not.toBeInTheDocument();
	});

	it('allows a public profile to keep its collection private', async () => {
		mocks.fetchPublicCollectionStats.mockRejectedValue(
			Object.assign(new Error('Resource not found'), { status: 404 })
		);

		render(PublicProfilePage);

		await waitFor(() => expect(screen.getByTestId('public-profile')).toBeInTheDocument());
		expect(screen.getByText('Die Sammlung ist privat.')).toBeInTheDocument();
		expect(screen.queryByRole('link', { name: 'Sammlung öffnen' })).not.toBeInTheDocument();
	});

	it('distinguishes an empty public collection from a private one', async () => {
		mocks.fetchPublicCollectionStats.mockResolvedValue({ ...stats, series_stats: [] });

		render(PublicProfilePage);

		await waitFor(() => expect(screen.getByTestId('public-profile')).toBeInTheDocument());
		expect(screen.getByText('Diese öffentliche Sammlung ist noch leer.')).toBeInTheDocument();
		expect(screen.queryByText('Die Sammlung ist privat.')).not.toBeInTheDocument();
	});

	it('reports unexpected collection-statistics failures instead of calling them private', async () => {
		mocks.fetchPublicCollectionStats.mockRejectedValue(new Error('Statistikfehler'));

		render(PublicProfilePage);

		await waitFor(() => expect(screen.getByRole('alert')).toHaveTextContent('Statistikfehler'));
		expect(screen.queryByText('Die Sammlung ist privat.')).not.toBeInTheDocument();
	});

	it('uses the fallback message for an untyped statistics failure', async () => {
		mocks.fetchPublicCollectionStats.mockRejectedValue('untyped failure');

		render(PublicProfilePage);

		await waitFor(() =>
			expect(screen.getByRole('alert')).toHaveTextContent(
				'Sammlungsstatistik konnte nicht geladen werden.'
			)
		);
	});

	it('uses the same not-found state for private and absent profiles', async () => {
		mocks.fetchPublicProfile.mockRejectedValue(
			Object.assign(new Error('Resource not found'), { status: 404 })
		);

		render(PublicProfilePage);

		await waitFor(() => expect(screen.getByTestId('private-profile')).toBeInTheDocument());
		expect(screen.getByText(/existiert nicht oder ist privat/)).toBeInTheDocument();
	});

	it('rejects invalid user ids locally', async () => {
		mocks.page.params.id = 'abc';

		render(PublicProfilePage);

		await waitFor(() => expect(screen.getByTestId('private-profile')).toBeInTheDocument());
		expect(mocks.fetchPublicProfile).not.toHaveBeenCalled();
	});

	it('rejects unsafe numeric user ids locally', async () => {
		mocks.page.params.id = '9007199254740992';

		render(PublicProfilePage);

		await waitFor(() => expect(screen.getByTestId('private-profile')).toBeInTheDocument());
		expect(mocks.fetchPublicProfile).not.toHaveBeenCalled();
	});

	it('reports unexpected public-profile failures', async () => {
		mocks.fetchPublicProfile.mockRejectedValue(new Error('Netzwerkfehler'));

		render(PublicProfilePage);

		await waitFor(() => expect(screen.getByRole('alert')).toHaveTextContent('Netzwerkfehler'));
	});

	it('uses the fallback message for an untyped public-profile failure', async () => {
		mocks.fetchPublicProfile.mockRejectedValue('untyped failure');

		render(PublicProfilePage);

		await waitFor(() =>
			expect(screen.getByRole('alert')).toHaveTextContent('Profil konnte nicht geladen werden.')
		);
	});
});

describe('Public Collection Page', () => {
	beforeEach(() => {
		vi.clearAllMocks();
		mocks.page.params.id = '7';
		mocks.fetchPublicCollection.mockResolvedValue({
			data: [collectionEntry()],
			page: 1,
			per_page: 100,
			total: 1
		});
	});

	it('shows condition, status and the full personal note in a public collection', async () => {
		render(PublicCollectionPage);

		await waitFor(() => expect(screen.getByTestId('public-collection-grid')).toBeInTheDocument());
		expect(screen.getByText('Dunkle Zukunft')).toBeInTheDocument();
		expect(screen.getByText('Vorhanden · Z2')).toBeInTheDocument();
		expect(screen.getByText('Exemplar 1')).toBeInTheDocument();
		expect(screen.getByTestId('collection-note')).toHaveTextContent('Erste Zeile Grüße 📚');
		expect(screen.getByTestId('collection-note')).toHaveClass('whitespace-pre-wrap');
	});

	it('shows edition and copy details when they are public', async () => {
		mocks.fetchPublicCollection.mockResolvedValue({
			data: [collectionEntry({ edition_label: '1. Auflage', copy_number: 2 })],
			page: 1,
			per_page: 100,
			total: 1
		});

		render(PublicCollectionPage);

		expect(await screen.findByText('1. Auflage · Exemplar 2')).toBeInTheDocument();
	});

	it('distinguishes multiple public copies even without edition labels', async () => {
		mocks.fetchPublicCollection.mockResolvedValue({
			data: [collectionEntry({ copy_number: 1 }), collectionEntry({ copy_number: 2 })],
			page: 1,
			per_page: 100,
			total: 2
		});

		render(PublicCollectionPage);

		await waitFor(() => expect(screen.getAllByTestId('public-collection-entry')).toHaveLength(2));
		expect(screen.getByText('Exemplar 1')).toBeInTheDocument();
		expect(screen.getByText('Exemplar 2')).toBeInTheDocument();
	});

	it('loads every public collection page', async () => {
		mocks.fetchPublicCollection
			.mockResolvedValueOnce({
				data: [collectionEntry()],
				page: 1,
				per_page: 100,
				total: 2
			})
			.mockResolvedValueOnce({
				data: [collectionEntry({ issue_id: 43, issue_number: 43, title: 'Das zweite Heft' })],
				page: 2,
				per_page: 100,
				total: 2
			});

		render(PublicCollectionPage);

		await waitFor(() => expect(screen.getAllByTestId('public-collection-entry')).toHaveLength(2));
		expect(mocks.fetchPublicCollection).toHaveBeenNthCalledWith(1, 7, 1, 100);
		expect(mocks.fetchPublicCollection).toHaveBeenNthCalledWith(2, 7, 2, 100);
	});

	it('stops pagination when the backend returns an empty partial page', async () => {
		mocks.fetchPublicCollection.mockResolvedValue({ data: [], page: 1, per_page: 100, total: 5 });

		render(PublicCollectionPage);

		await waitFor(() => expect(screen.getByTestId('public-collection-empty')).toBeInTheDocument());
		expect(mocks.fetchPublicCollection).toHaveBeenCalledOnce();
	});

	it('prefers local covers, falls back to remote covers and renders missing-cover placeholders', async () => {
		mocks.fetchPublicCollection.mockResolvedValue({
			data: [
				collectionEntry({ cover_url: 'https://example.com/remote.jpg' }),
				collectionEntry({
					issue_id: 43,
					issue_number: 43,
					title: 'Lokales Cover',
					cover_local_path: '/covers/local.jpg',
					cover_url: 'https://example.com/ignored.jpg'
				}),
				collectionEntry({ issue_id: 44, issue_number: 44, title: 'Ohne Cover' })
			],
			page: 1,
			per_page: 100,
			total: 3
		});

		render(PublicCollectionPage);

		await waitFor(() => expect(screen.getAllByTestId('public-collection-entry')).toHaveLength(3));
		expect(screen.getByAltText(/Dunkle Zukunft/)).toHaveAttribute(
			'src',
			'https://example.com/remote.jpg'
		);
		expect(screen.getByAltText(/Lokales Cover/)).toHaveAttribute('src', '/covers/local.jpg');
		expect(screen.getByText('#44')).toBeInTheDocument();
	});

	it('renders every persisted status and an empty note explicitly', async () => {
		mocks.fetchPublicCollection.mockResolvedValue({
			data: [
				collectionEntry({ status: 'duplicate', notes: null }),
				collectionEntry({
					issue_id: 43,
					issue_number: 43,
					title: 'Gesuchtes Heft',
					status: 'wanted',
					condition_grade: null,
					notes: ''
				})
			],
			page: 1,
			per_page: 100,
			total: 2
		});

		render(PublicCollectionPage);

		await waitFor(() => expect(screen.getByText('Doppelt/Tauschbar · Z2')).toBeInTheDocument());
		expect(screen.getByText('Gesucht')).toBeInTheDocument();
		expect(screen.queryByText(/Gesucht ·/)).not.toBeInTheDocument();
		expect(screen.getAllByText('Keine öffentliche Notiz.')).toHaveLength(2);
	});

	it('shows an empty state for a public collection without entries', async () => {
		mocks.fetchPublicCollection.mockResolvedValue({ data: [], page: 1, per_page: 100, total: 0 });

		render(PublicCollectionPage);

		await waitFor(() => expect(screen.getByTestId('public-collection-empty')).toBeInTheDocument());
	});

	it('uses the same not-found state for private and absent collections', async () => {
		mocks.fetchPublicCollection.mockRejectedValue(
			Object.assign(new Error('Resource not found'), { status: 404 })
		);

		render(PublicCollectionPage);

		await waitFor(() => expect(screen.getByTestId('private-collection')).toBeInTheDocument());
		expect(screen.getByText(/existiert nicht oder ist privat/)).toBeInTheDocument();
	});

	it('rejects invalid ids locally and reports unexpected failures', async () => {
		mocks.page.params.id = '0';
		const invalidView = render(PublicCollectionPage);
		await waitFor(() => expect(screen.getByTestId('private-collection')).toBeInTheDocument());
		expect(mocks.fetchPublicCollection).not.toHaveBeenCalled();
		invalidView.unmount();

		mocks.page.params.id = '7';
		mocks.fetchPublicCollection.mockRejectedValueOnce(new Error('Netzwerkfehler'));
		render(PublicCollectionPage);
		await waitFor(() => expect(screen.getByRole('alert')).toHaveTextContent('Netzwerkfehler'));
	});

	it('uses the fallback message for an untyped public-collection failure', async () => {
		mocks.fetchPublicCollection.mockRejectedValue('untyped failure');

		render(PublicCollectionPage);

		await waitFor(() =>
			expect(screen.getByRole('alert')).toHaveTextContent('Sammlung konnte nicht geladen werden.')
		);
	});
});
