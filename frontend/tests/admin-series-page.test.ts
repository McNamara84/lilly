import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/svelte';
import { userEvent } from '@testing-library/user-event';
import AdminSeriesPage from '../src/routes/admin/series/+page.svelte';

vi.mock('$lib/api/admin', () => ({
	fetchAllSeries: vi.fn(),
	deactivateSeries: vi.fn()
}));

vi.mock('$app/paths', () => ({
	resolve: (path: string) => path
}));

import { fetchAllSeries, deactivateSeries } from '$lib/api/admin';

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
		source_url: 'https://maddraxikon.de'
	},
	{
		id: 2,
		name: 'Dorian Hunter',
		slug: 'dorian-hunter',
		publisher: 'Bastei',
		genre: 'Horror',
		frequency: null,
		total_issues: 60,
		status: 'completed',
		active: false,
		latest_import_job_id: 7,
		source_url: null
	}
];

describe('Admin Series Page', () => {
	beforeEach(() => {
		vi.clearAllMocks();
	});

	it('shows loading state initially', () => {
		vi.mocked(fetchAllSeries).mockReturnValue(new Promise(() => {}));
		render(AdminSeriesPage);

		expect(screen.getByTestId('loading-indicator')).toHaveTextContent('Lade Serien...');
	});

	it('renders series table after loading', async () => {
		vi.mocked(fetchAllSeries).mockResolvedValue(mockSeries);
		render(AdminSeriesPage);

		await waitFor(() => {
			expect(screen.getByTestId('series-table')).toBeInTheDocument();
		});

		expect(screen.getByText('Maddrax')).toBeInTheDocument();
		expect(screen.getByText('Dorian Hunter')).toBeInTheDocument();
	});

	it('shows empty state when no series exist', async () => {
		vi.mocked(fetchAllSeries).mockResolvedValue([]);
		render(AdminSeriesPage);

		await waitFor(() => {
			expect(screen.getByTestId('empty-state')).toBeInTheDocument();
		});
	});

	it('shows error message on fetch failure', async () => {
		vi.mocked(fetchAllSeries).mockRejectedValue(new Error('Connection refused'));
		render(AdminSeriesPage);

		await waitFor(() => {
			expect(screen.getByTestId('error-message')).toHaveTextContent('Connection refused');
		});
	});

	it('displays active/inactive status indicators', async () => {
		vi.mocked(fetchAllSeries).mockResolvedValue(mockSeries);
		render(AdminSeriesPage);

		await waitFor(() => {
			expect(screen.getByTestId('series-table')).toBeInTheDocument();
		});

		const activeLabels = screen.getAllByLabelText('Aktiv');
		const inactiveLabels = screen.getAllByLabelText('Inaktiv');
		expect(activeLabels).toHaveLength(1);
		expect(inactiveLabels).toHaveLength(1);
	});

	it('shows deactivation for active series and review navigation for inactive series', async () => {
		vi.mocked(fetchAllSeries).mockResolvedValue(mockSeries);
		render(AdminSeriesPage);

		await waitFor(() => {
			expect(screen.getByTestId('series-table')).toBeInTheDocument();
		});

		const buttons = screen.getAllByTestId('toggle-active-button');
		expect(buttons[0]).toHaveTextContent('Deaktivieren');
		expect(screen.getByTestId('review-import-link')).toHaveTextContent('Import prüfen');
		expect(screen.getByTestId('review-import-link')).toHaveAttribute('href', '/admin/import/7');
	});

	it('calls deactivateSeries when clicking deactivate button', async () => {
		vi.mocked(fetchAllSeries).mockResolvedValue(mockSeries);
		vi.mocked(deactivateSeries).mockResolvedValue(undefined);
		const user = userEvent.setup();
		render(AdminSeriesPage);

		await waitFor(() => {
			expect(screen.getByTestId('series-table')).toBeInTheDocument();
		});

		const buttons = screen.getAllByTestId('toggle-active-button');
		await user.click(buttons[0]);

		expect(deactivateSeries).toHaveBeenCalledWith('maddrax');
	});

	it('does not expose direct activation for an inactive series', async () => {
		vi.mocked(fetchAllSeries).mockResolvedValue(mockSeries);
		render(AdminSeriesPage);

		await waitFor(() => {
			expect(screen.getByTestId('series-table')).toBeInTheDocument();
		});

		expect(screen.getAllByTestId('toggle-active-button')).toHaveLength(1);
		expect(screen.getByTestId('review-import-link')).toHaveAttribute('href', '/admin/import/7');
	});

	it('links to import creation when an inactive series has no reviewable job', async () => {
		vi.mocked(fetchAllSeries).mockResolvedValue([{ ...mockSeries[1], latest_import_job_id: null }]);
		render(AdminSeriesPage);

		await waitFor(() => expect(screen.getByTestId('review-import-link')).toBeInTheDocument());
		expect(screen.getByTestId('review-import-link')).toHaveTextContent('Import starten');
		expect(screen.getByTestId('review-import-link')).toHaveAttribute('href', '/admin/import');
	});

	it('displays total issue count', async () => {
		vi.mocked(fetchAllSeries).mockResolvedValue(mockSeries);
		render(AdminSeriesPage);

		await waitFor(() => {
			expect(screen.getByTestId('series-table')).toBeInTheDocument();
		});

		expect(screen.getByText('620')).toBeInTheDocument();
		expect(screen.getByText('60')).toBeInTheDocument();
	});

	it('has page title', () => {
		vi.mocked(fetchAllSeries).mockReturnValue(new Promise(() => {}));
		render(AdminSeriesPage);

		expect(screen.getByTestId('admin-series-title')).toHaveTextContent('Serien-Verwaltung');
	});
});
