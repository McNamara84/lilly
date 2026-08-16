import { cleanup, render, screen, waitFor } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
	status: {
		online: true,
		syncing: false,
		pendingCount: 0,
		conflictCount: 0,
		lastSyncedAt: null as string | null,
		syncError: null as string | null
	},
	pwa: { canInstall: false, updateAvailable: false },
	getCachedProfile: vi.fn(),
	listConflicts: vi.fn(),
	acceptConflictServerVersion: vi.fn(),
	reapplyConflictLocalVersion: vi.fn(),
	refreshOfflineStatus: vi.fn(),
	synchronizeNow: vi.fn(),
	promptInstall: vi.fn(),
	activateWaitingServiceWorker: vi.fn()
}));

vi.mock('$lib/offline/database', () => ({ getCachedProfile: mocks.getCachedProfile }));
vi.mock('$lib/offline/collection', () => ({
	listConflicts: mocks.listConflicts,
	acceptConflictServerVersion: mocks.acceptConflictServerVersion,
	reapplyConflictLocalVersion: mocks.reapplyConflictLocalVersion
}));
vi.mock('$lib/offline/status.svelte', () => ({
	getOfflineStatus: () => mocks.status,
	formatOfflineStatusLabel: () =>
		mocks.status.conflictCount > 0 ? `${mocks.status.conflictCount} Konflikt(e)` : 'Synchronisiert',
	refreshOfflineStatus: mocks.refreshOfflineStatus,
	synchronizeNow: mocks.synchronizeNow
}));
vi.mock('$lib/offline/pwa.svelte', () => ({
	getPwaState: () => mocks.pwa,
	promptInstall: mocks.promptInstall,
	activateWaitingServiceWorker: mocks.activateWaitingServiceWorker
}));

import OfflineStatus from '$lib/components/offline/OfflineStatus.svelte';

const serverEntry = {
	id: 30,
	issue_id: 20,
	issue_number: 1,
	title: 'Der Gott aus dem Eis',
	series_id: 10,
	series_name: 'Maddrax',
	series_slug: 'maddrax',
	cover_url: null,
	cover_local_path: null,
	copy_number: 1,
	edition_label: null,
	condition_grade: 'Z3',
	status: 'owned',
	notes: null,
	revision: 2,
	created_at: '2026-08-15T01:00:00Z',
	updated_at: '2026-08-15T01:00:00Z'
};

const updateConflict = {
	mutation_id: 'update-conflict',
	user_id: 1,
	created_at: '2026-08-15T02:00:00Z',
	mutation: {
		mutation_id: 'update-conflict',
		user_id: 1,
		created_at: '2026-08-15T02:00:00Z',
		attempts: 0,
		last_error: null,
		operation: 'update',
		entry_id: 30,
		base_revision: 1,
		changes: { status: 'duplicate', condition_grade: 'Z1' }
	},
	server_entry: serverEntry,
	error: 'Der Serverstand ist neuer.',
	code: 'collection_revision_conflict',
	status: 'conflict'
} as const;

const createConflict = {
	mutation_id: 'create-conflict',
	user_id: 1,
	created_at: '2026-08-15T02:01:00Z',
	mutation: {
		mutation_id: 'create-conflict',
		user_id: 1,
		created_at: '2026-08-15T02:01:00Z',
		attempts: 0,
		last_error: null,
		operation: 'create',
		temp_entry_id: -1,
		entry: { issue_id: 20, condition_grade: null, status: 'owned' }
	},
	server_entry: null,
	error: 'Der Eintrag konnte nicht angelegt werden.',
	code: null,
	status: 'rejected'
} as const;

describe('OfflineStatus', () => {
	beforeEach(() => {
		vi.clearAllMocks();
		Object.assign(mocks.status, {
			online: true,
			syncing: false,
			pendingCount: 0,
			conflictCount: 0,
			lastSyncedAt: null,
			syncError: null
		});
		Object.assign(mocks.pwa, { canInstall: false, updateAvailable: false });
		mocks.getCachedProfile.mockResolvedValue({ id: 1 });
		mocks.listConflicts.mockResolvedValue([]);
		mocks.acceptConflictServerVersion.mockResolvedValue(undefined);
		mocks.reapplyConflictLocalVersion.mockResolvedValue(undefined);
		mocks.refreshOfflineStatus.mockResolvedValue(undefined);
		mocks.synchronizeNow.mockResolvedValue(undefined);
		mocks.promptInstall.mockResolvedValue(undefined);
		mocks.activateWaitingServiceWorker.mockResolvedValue(undefined);
	});

	afterEach(cleanup);

	it('offers installation, activation and manual synchronization actions', async () => {
		Object.assign(mocks.pwa, { canInstall: true, updateAvailable: true });
		mocks.status.lastSyncedAt = '2026-08-15T02:03:04Z';
		render(OfflineStatus);

		await userEvent.click(screen.getByTestId('pwa-install-button'));
		await userEvent.click(screen.getByRole('button', { name: 'Jetzt aktualisieren' }));
		await userEvent.click(screen.getByTestId('offline-status'));

		expect(mocks.promptInstall).toHaveBeenCalledOnce();
		expect(mocks.activateWaitingServiceWorker).toHaveBeenCalledOnce();
		expect(mocks.synchronizeNow).toHaveBeenCalledOnce();
		expect(screen.getByTestId('offline-status').title).toContain('Letzter Datenstand:');
	});

	it('shows both conflict variants and applies both resolution strategies', async () => {
		mocks.status.conflictCount = 2;
		mocks.listConflicts.mockResolvedValue([updateConflict, createConflict]);
		render(OfflineStatus);

		await userEvent.click(screen.getByTestId('offline-status'));
		expect(await screen.findByTestId('conflict-panel')).toHaveTextContent('Server');
		expect(screen.getByTestId('conflict-panel')).toHaveTextContent('Kein Eintrag');
		expect(screen.getByTestId('conflict-panel')).toHaveTextContent('duplicate');
		expect(screen.getByTestId('conflict-panel')).toHaveTextContent('owned');

		await userEvent.click(screen.getByRole('button', { name: 'Lokalen Stand erneut anwenden' }));
		expect(mocks.reapplyConflictLocalVersion).toHaveBeenCalledWith(updateConflict);
		expect(mocks.synchronizeNow).toHaveBeenCalledOnce();

		await userEvent.click(screen.getByRole('button', { name: 'Serverstand übernehmen' }));
		expect(mocks.acceptConflictServerVersion).toHaveBeenCalledWith(createConflict);
		expect(mocks.refreshOfflineStatus).toHaveBeenCalledOnce();
		await waitFor(() => expect(screen.queryByTestId('conflict-panel')).not.toBeInTheDocument());

		await userEvent.click(screen.getByTestId('offline-status'));
		expect(mocks.listConflicts).toHaveBeenCalledOnce();
	});

	it('handles an unavailable cached profile without opening an empty conflict panel', async () => {
		mocks.status.conflictCount = 1;
		mocks.getCachedProfile.mockRejectedValue(new Error('IndexedDB unavailable'));
		render(OfflineStatus);

		await userEvent.click(screen.getByTestId('offline-status'));

		expect(mocks.listConflicts).not.toHaveBeenCalled();
		expect(screen.queryByTestId('conflict-panel')).not.toBeInTheDocument();
	});
});
