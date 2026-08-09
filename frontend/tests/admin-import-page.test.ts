import { describe, it, expect, vi, beforeEach } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { userEvent } from '@testing-library/user-event';
import AdminImportPage from '../src/routes/admin/import/+page.svelte';

vi.mock('$lib/api/admin', () => ({
	fetchAdapters: vi.fn(),
	startImport: vi.fn(),
	fetchImportHistory: vi.fn(),
	fetchImportSchedule: vi.fn()
}));

vi.mock('$app/navigation', () => ({
	goto: vi.fn()
}));

vi.mock('$app/paths', () => ({
	resolve: vi.fn((path: string) => path)
}));

import {
	fetchAdapters,
	startImport,
	fetchImportHistory,
	fetchImportSchedule
} from '$lib/api/admin';
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
		trigger_type: 'manual' as const,
		scheduled_for: null,
		status: 'completed',
		total_issues: 620,
		imported_issues: 620,
		failed_issues: 0,
		error_message: null,
		started_by: 1,
		started_at: '2025-06-01T10:00:00Z',
		completed_at: '2025-06-01T10:15:00Z'
	}
];

describe('Admin Import Page', () => {
	beforeEach(() => {
		vi.clearAllMocks();
		vi.mocked(fetchImportSchedule).mockResolvedValue({
			enabled: true,
			schedule: '0 10 6 * * Sat *',
			timezone: 'Europe/Berlin',
			adapters: ['maddrax', 'john-sinclair'],
			next_run: '2026-08-08T04:10:00Z'
		});
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
		const user = userEvent.setup();
		render(AdminImportPage);

		await waitFor(() => {
			expect(screen.getByTestId('adapter-select')).toBeInTheDocument();
		});

		const select = screen.getByTestId('adapter-select') as HTMLSelectElement;
		expect(select.options).toHaveLength(2);

		await user.selectOptions(select, 'gruselroman');
		expect(select).toHaveValue('gruselroman');
	});

	it('does not start an import when no adapter is available', async () => {
		vi.mocked(fetchAdapters).mockResolvedValue([]);
		vi.mocked(fetchImportHistory).mockResolvedValue([]);
		render(AdminImportPage);

		await waitFor(() => {
			expect(screen.getByTestId('start-import-button')).toBeDisabled();
		});

		await fireEvent.click(screen.getByTestId('start-import-button'));
		expect(startImport).not.toHaveBeenCalled();
	});

	it('shows error on fetch failure', async () => {
		vi.mocked(fetchAdapters).mockRejectedValue(new Error('Network error'));
		vi.mocked(fetchImportHistory).mockResolvedValue([]);
		render(AdminImportPage);

		await waitFor(() => {
			expect(screen.getByTestId('error-message')).toHaveTextContent('Network error');
		});
	});

	it('shows a generic error when loading rejects with a non-Error value', async () => {
		vi.mocked(fetchAdapters).mockRejectedValue('unexpected');
		vi.mocked(fetchImportHistory).mockResolvedValue([]);
		render(AdminImportPage);

		await waitFor(() => {
			expect(screen.getByTestId('error-message')).toHaveTextContent('Failed to load data');
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
			trigger_type: 'manual',
			scheduled_for: null,
			status: 'running',
			total_issues: 0,
			imported_issues: 0,
			failed_issues: 0,
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

	it('shows a generic import error for a non-Error rejection', async () => {
		vi.mocked(fetchAdapters).mockResolvedValue(mockAdapters);
		vi.mocked(fetchImportHistory).mockResolvedValue([]);
		vi.mocked(startImport).mockRejectedValue('unexpected');

		const user = userEvent.setup();
		render(AdminImportPage);

		await waitFor(() => {
			expect(screen.getByTestId('start-import-button')).toBeInTheDocument();
		});

		await user.click(screen.getByTestId('start-import-button'));

		await waitFor(() => {
			expect(screen.getByTestId('error-message')).toHaveTextContent('Import failed');
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

	it('shows scheduled history entries and handles a missing start time', async () => {
		vi.mocked(fetchAdapters).mockResolvedValue(mockAdapters);
		vi.mocked(fetchImportHistory).mockResolvedValue([
			{
				...mockHistory[0],
				id: 2,
				trigger_type: 'scheduled',
				scheduled_for: '2026-08-08T04:10:00Z',
				status: 'completed_with_errors',
				total_issues: 10,
				imported_issues: 8,
				failed_issues: 2,
				started_by: null,
				started_at: null
			}
		]);
		render(AdminImportPage);

		await waitFor(() => {
			expect(screen.getByTestId('history-table')).toBeInTheDocument();
		});

		const row = screen.getByTestId('history-row');
		expect(row).toHaveTextContent('Automatisch');
		expect(row).toHaveTextContent('completed_with_errors');
		expect(row).toHaveTextContent('10 / 10');
		expect(row).toHaveTextContent('–');
	});

	it('renders every non-completed import status in the history', async () => {
		vi.mocked(fetchAdapters).mockResolvedValue(mockAdapters);
		vi.mocked(fetchImportHistory).mockResolvedValue(
			(['failed', 'running', 'pending', 'interrupted', 'cancelled'] as const).map(
				(status, index) => ({
					...mockHistory[0],
					id: index + 3,
					status
				})
			)
		);
		render(AdminImportPage);

		await waitFor(() => {
			expect(screen.getByText('failed')).toBeInTheDocument();
		});
		expect(screen.getByText('running')).toBeInTheDocument();
		expect(screen.getByText('pending')).toBeInTheDocument();
		expect(screen.getByText('interrupted')).toBeInTheDocument();
		expect(screen.getByText('cancelled')).toBeInTheDocument();
	});

	it('shows source, retry origin and skipped issues in the history', async () => {
		vi.mocked(fetchAdapters).mockResolvedValue(mockAdapters);
		vi.mocked(fetchImportHistory).mockResolvedValue([
			{
				...mockHistory[0],
				total_issues: 10,
				imported_issues: 7,
				skipped_issues: 1,
				failed_issues: 2,
				source_key: 'maddraxikon',
				retry_of_job_id: 41
			}
		]);
		render(AdminImportPage);

		await waitFor(() => expect(screen.getByTestId('history-row')).toBeInTheDocument());

		const row = screen.getByTestId('history-row');
		expect(row).toHaveTextContent('maddraxikon');
		expect(row).toHaveTextContent('Retry von #41');
		expect(row).toHaveTextContent('10 / 10');
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

	it('shows the enabled weekly scheduler and its timezone', async () => {
		vi.mocked(fetchAdapters).mockResolvedValue(mockAdapters);
		vi.mocked(fetchImportHistory).mockResolvedValue([]);
		render(AdminImportPage);

		await waitFor(() => {
			expect(screen.getByTestId('schedule-status')).toHaveTextContent('Aktiv für');
		});
		expect(screen.getByTestId('schedule-status')).toHaveTextContent('Europe/Berlin');
	});

	it('formats the next run in the configured scheduler timezone', async () => {
		vi.mocked(fetchImportSchedule).mockResolvedValue({
			enabled: true,
			schedule: '0 10 6 * * Sat *',
			timezone: 'UTC',
			adapters: ['john-sinclair'],
			next_run: '2026-08-08T04:10:00Z'
		});
		vi.mocked(fetchAdapters).mockResolvedValue(mockAdapters);
		vi.mocked(fetchImportHistory).mockResolvedValue([]);
		render(AdminImportPage);

		await waitFor(() => {
			expect(screen.getByTestId('schedule-status')).toHaveTextContent('UTC');
		});
		expect(screen.getByTestId('schedule-status')).toHaveTextContent('04:10:00');
		expect(screen.getByTestId('schedule-status')).not.toHaveTextContent('06:10:00');
	});

	it('shows the disabled scheduler with its configured cron expression', async () => {
		vi.mocked(fetchImportSchedule).mockResolvedValue({
			enabled: false,
			schedule: '0 30 9 * * Mon-Fri *',
			timezone: 'Europe/Berlin',
			adapters: ['maddrax', 'john-sinclair'],
			next_run: null
		});
		vi.mocked(fetchAdapters).mockResolvedValue(mockAdapters);
		vi.mocked(fetchImportHistory).mockResolvedValue([]);
		render(AdminImportPage);

		await waitFor(() => {
			expect(screen.getByTestId('schedule-status')).toHaveTextContent('Deaktiviert');
		});
		expect(screen.getByTestId('schedule-status')).toHaveTextContent('0 30 9 * * Mon-Fri *');
		expect(screen.getByTestId('schedule-status')).not.toHaveTextContent('samstags um 06:10 Uhr');
		expect(screen.getByTestId('schedule-status')).toHaveTextContent('Europe/Berlin');
	});
});
