import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/svelte';
import { userEvent } from '@testing-library/user-event';
import AdminImportPage from '../src/routes/admin/import/+page.svelte';

vi.mock('$lib/api/admin', () => ({
	fetchAdapters: vi.fn(),
	startImport: vi.fn(),
	fetchImportHistory: vi.fn()
}));

vi.mock('$app/navigation', () => ({
	goto: vi.fn()
}));

vi.mock('$app/paths', () => ({
	resolve: vi.fn((path: string) => path)
}));

import { fetchAdapters, startImport, fetchImportHistory } from '$lib/api/admin';
import { goto } from '$app/navigation';

const mockAdapters = [
	{ name: 'maddrax', display_name: 'Maddrax Wiki', version: '0.1.0' },
	{ name: 'gruselroman', display_name: 'Gruselroman Wiki', version: '0.1.0' }
];

const mockHistory = [
	{
		id: 1,
		series_id: 1,
		series_slug: 'maddrax',
		adapter_name: 'maddrax',
		status: 'completed',
		total_issues: 620,
		imported_issues: 620,
		error_message: null,
		started_by: 1,
		started_at: '2025-06-01T10:00:00Z',
		completed_at: '2025-06-01T10:15:00Z'
	}
];

describe('Admin Import Page', () => {
	beforeEach(() => {
		vi.clearAllMocks();
	});

	it('shows loading state initially', () => {
		vi.mocked(fetchAdapters).mockReturnValue(new Promise(() => {}));
		vi.mocked(fetchImportHistory).mockReturnValue(new Promise(() => {}));
		render(AdminImportPage);

		expect(screen.getByTestId('loading-indicator')).toHaveTextContent('Lade...');
	});

	it('renders adapter select and import button after loading', async () => {
		vi.mocked(fetchAdapters).mockResolvedValue(mockAdapters);
		vi.mocked(fetchImportHistory).mockResolvedValue(mockHistory);
		render(AdminImportPage);

		await waitFor(() => {
			expect(screen.getByTestId('adapter-select')).toBeInTheDocument();
		});

		expect(screen.getByTestId('start-import-button')).toBeInTheDocument();
	});

	it('populates adapter select with options', async () => {
		vi.mocked(fetchAdapters).mockResolvedValue(mockAdapters);
		vi.mocked(fetchImportHistory).mockResolvedValue([]);
		render(AdminImportPage);

		await waitFor(() => {
			expect(screen.getByTestId('adapter-select')).toBeInTheDocument();
		});

		const select = screen.getByTestId('adapter-select') as HTMLSelectElement;
		expect(select.options).toHaveLength(2);
	});

	it('shows error on fetch failure', async () => {
		vi.mocked(fetchAdapters).mockRejectedValue(new Error('Network error'));
		vi.mocked(fetchImportHistory).mockResolvedValue([]);
		render(AdminImportPage);

		await waitFor(() => {
			expect(screen.getByTestId('error-message')).toHaveTextContent('Network error');
		});
	});

	it('starts import and navigates to detail page', async () => {
		vi.mocked(fetchAdapters).mockResolvedValue(mockAdapters);
		vi.mocked(fetchImportHistory).mockResolvedValue([]);
		vi.mocked(startImport).mockResolvedValue({
			id: 5,
			series_id: 1,
			series_slug: 'maddrax',
			adapter_name: 'maddrax',
			status: 'running',
			total_issues: 0,
			imported_issues: 0,
			error_message: null,
			started_by: 1,
			started_at: '2025-06-01T10:00:00Z',
			completed_at: null
		});

		const user = userEvent.setup();
		render(AdminImportPage);

		await waitFor(() => {
			expect(screen.getByTestId('start-import-button')).toBeInTheDocument();
		});

		await user.click(screen.getByTestId('start-import-button'));

		expect(startImport).toHaveBeenCalledWith('maddrax');
		await waitFor(() => {
			expect(goto).toHaveBeenCalledWith('/admin/import/5');
		});
	});

	it('shows import error message', async () => {
		vi.mocked(fetchAdapters).mockResolvedValue(mockAdapters);
		vi.mocked(fetchImportHistory).mockResolvedValue([]);
		vi.mocked(startImport).mockRejectedValue(new Error('Import already running'));

		const user = userEvent.setup();
		render(AdminImportPage);

		await waitFor(() => {
			expect(screen.getByTestId('start-import-button')).toBeInTheDocument();
		});

		await user.click(screen.getByTestId('start-import-button'));

		await waitFor(() => {
			expect(screen.getByTestId('error-message')).toHaveTextContent('Import already running');
		});
	});

	it('shows import history table', async () => {
		vi.mocked(fetchAdapters).mockResolvedValue(mockAdapters);
		vi.mocked(fetchImportHistory).mockResolvedValue(mockHistory);
		render(AdminImportPage);

		await waitFor(() => {
			expect(screen.getByTestId('history-table')).toBeInTheDocument();
		});

		expect(screen.getByText('maddrax')).toBeInTheDocument();
		expect(screen.getByText('completed')).toBeInTheDocument();
	});

	it('shows empty history message', async () => {
		vi.mocked(fetchAdapters).mockResolvedValue(mockAdapters);
		vi.mocked(fetchImportHistory).mockResolvedValue([]);
		render(AdminImportPage);

		await waitFor(() => {
			expect(screen.getByTestId('start-import-section')).toBeInTheDocument();
		});

		expect(screen.getByTestId('empty-history')).toBeInTheDocument();
	});

	it('has page title', async () => {
		vi.mocked(fetchAdapters).mockResolvedValue([]);
		vi.mocked(fetchImportHistory).mockResolvedValue([]);
		render(AdminImportPage);

		expect(screen.getByTestId('admin-import-title')).toHaveTextContent('Import');
	});
});
