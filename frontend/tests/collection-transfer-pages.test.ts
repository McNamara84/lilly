import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import CollectionImport from '../src/routes/collection/import/+page.svelte';
import PersonalExport from '../src/routes/account/data-export/+page.svelte';
import type { TransferJob } from '#lib/api/collection-transfer.js';
import { TransferError } from '#lib/api/collection-transfer.js';

const mocks = vi.hoisted(() => ({
	auth: { isLoading: false, isAuthenticated: true, isOfflineSession: false },
	offline: { online: true },
	goto: vi.fn(),
	preview: vi.fn(),
	commit: vi.fn(),
	fetchImport: vi.fn(),
	refresh: vi.fn(),
	download: vi.fn(),
	options: vi.fn(),
	list: vi.fn(),
	request: vi.fn(),
	personalDownload: vi.fn(),
	password: vi.fn(),
	oauth: vi.fn()
}));
vi.mock('$app/navigation', () => ({ goto: mocks.goto }));
vi.mock('$app/paths', () => ({ resolve: (path: string) => `/${path}` }));
vi.mock('#lib/stores/auth.svelte.js', () => ({ getAuthState: () => mocks.auth }));
vi.mock('#lib/offline/status.svelte.js', () => ({ getOfflineStatus: () => mocks.offline }));
vi.mock('#lib/api/auth.js', () => ({ startOAuth: mocks.oauth }));
vi.mock('#lib/api/account-erasure.js', () => ({ reauthenticateWithPassword: mocks.password }));
vi.mock('#lib/api/collection-transfer.js', () => ({
	previewCollectionImport: mocks.preview,
	commitCollectionImport: mocks.commit,
	fetchCollectionImport: mocks.fetchImport,
	refreshImportedCollection: mocks.refresh,
	downloadCollection: mocks.download,
	fetchReauthenticationOptions: mocks.options,
	listPersonalExports: mocks.list,
	requestPersonalExport: mocks.request,
	downloadPersonalExport: mocks.personalDownload,
	TransferError: class extends Error {
		code?: string;
		constructor(message: string, code?: string) {
			super(message);
			this.code = code;
		}
	},
	transferStatusLabels: {
		prepared: 'Vorschau bereit',
		queued: 'In Warteschlange',
		running: 'Wird verarbeitet',
		completed: 'Import abgeschlossen',
		ready: 'Download bereit',
		failed: 'Fehlgeschlagen',
		cancelled: 'Nicht mehr gültig',
		expired: 'Abgelaufen'
	},
	rowStatusLabels: {
		new: 'Neu',
		created: 'Hinzugefügt',
		unchanged: 'Übersprungen',
		conflict: 'Konflikt',
		invalid: 'Ungültig',
		failed: 'Fehlgeschlagen'
	},
	transferRowReason: (code: string | null) =>
		code === 'entry_changed' ? 'Vorhandene Daten bleiben erhalten.' : ''
}));

function job(overrides: Partial<TransferJob> = {}): TransferJob {
	return {
		id: 'job-1',
		kind: 'import',
		format: overrides.kind === 'export' ? 'zip' : 'json',
		status: 'prepared',
		created_at: '2026-11-22T12:00:00Z',
		completed_at: null,
		snapshot_at: null,
		expires_at: '2099-11-23T12:00:00Z',
		error_code: null,
		total_rows: 3,
		counts: { new: 1, created: 0, unchanged: 0, conflict: 1, invalid: 1, failed: 0 },
		results: [
			{ row_number: 1, copy_key: 'first', status: 'new', code: null, entry_id: null },
			{ row_number: 2, copy_key: 'second', status: 'conflict', code: 'entry_changed', entry_id: 2 }
		],
		page: 1,
		per_page: 100,
		byte_size: 0,
		...overrides
	};
}

beforeEach(() => {
	vi.resetAllMocks();
	window.history.replaceState(null, '', '/');
	Object.assign(mocks.auth, { isLoading: false, isAuthenticated: true, isOfflineSession: false });
	mocks.offline.online = true;
	mocks.preview.mockResolvedValue(job());
	mocks.commit.mockResolvedValue(job({ status: 'queued' }));
	mocks.fetchImport.mockResolvedValue(job({ status: 'completed' }));
	mocks.refresh.mockResolvedValue(true);
	mocks.download.mockResolvedValue(undefined);
	mocks.personalDownload.mockResolvedValue(undefined);
	mocks.options.mockResolvedValue({
		recent_authentication: true,
		password: true,
		google: false,
		github: false
	});
	mocks.list.mockResolvedValue([]);
	mocks.request.mockResolvedValue(job({ kind: 'export', status: 'queued' }));
	mocks.password.mockResolvedValue(undefined);
});
afterEach(() => {
	cleanup();
	sessionStorage.removeItem('lilly-data-export-reauth');
	vi.restoreAllMocks();
});

async function upload() {
	const file = new File(['{}'], 'bestand.json', { type: 'application/json' });
	await fireEvent.change(screen.getByLabelText('Dateiformat'), { target: { value: 'json' } });
	await fireEvent.change(screen.getByLabelText('Bestandsdatei'), { target: { files: [file] } });
	await fireEvent.click(screen.getByTestId('collection-import-preview'));
	return file;
}

function settle<T>(request: PromiseWithResolvers<T>, result: T, outcome: 'success' | 'failure') {
	return act(() => {
		if (outcome === 'failure') request.reject(new Error('Request failed'));
		else request.resolve(result);
	});
}

describe('collection import and export page', () => {
	it.each(['csv', 'json'] as const)('restores a %s job from its protected URL', async (format) => {
		window.history.replaceState(null, '', '/collection/import?job=job-1');
		mocks.fetchImport.mockResolvedValue(job({ status: 'completed', format }));
		render(CollectionImport);
		await screen.findByRole('heading', { name: 'Import abgeschlossen' });
		expect(mocks.fetchImport).toHaveBeenCalledWith('job-1');
		expect(screen.getByLabelText('Dateiformat')).toHaveValue(format);
		expect(mocks.preview).not.toHaveBeenCalled();
	});
	it.each([
		[new Error('Auftrag nicht verfügbar'), 'Auftrag nicht verfügbar'],
		[null, 'Der Auftrag konnte nicht geladen werden.']
	])('keeps a fresh upload possible when restoration fails with %s', async (cause, message) => {
		window.history.replaceState(null, '', '/collection/import?job=missing');
		mocks.fetchImport.mockRejectedValue(cause);
		render(CollectionImport);
		expect(await screen.findByRole('alert')).toHaveTextContent(message);
		expect(screen.getByLabelText('Bestandsdatei')).not.toBeDisabled();
	});
	it('waits for session verification before restoring a private import', () => {
		Object.assign(mocks.auth, { isLoading: true, isAuthenticated: false });
		window.history.replaceState(null, '', '/collection/import?job=private');
		render(CollectionImport);
		expect(screen.getByLabelText('Bestandsdatei')).toBeDisabled();
		expect(screen.getByTestId('collection-export-json')).toBeDisabled();
		expect(mocks.fetchImport).not.toHaveBeenCalled();
		expect(mocks.goto).not.toHaveBeenCalled();
	});
	it.each(['success', 'failure'] as const)(
		'keeps a restored import from refreshing private data after leaving (%s)',
		async (outcome) => {
			window.history.replaceState(null, '', '/collection/import?job=job-1');
			const response = Promise.withResolvers<TransferJob>();
			mocks.fetchImport.mockReturnValueOnce(response.promise);
			const { unmount } = render(CollectionImport);
			await waitFor(() => expect(mocks.fetchImport).toHaveBeenCalledWith('job-1'));
			unmount();
			await settle(response, job({ status: 'completed' }), outcome);
			expect(mocks.refresh).not.toHaveBeenCalled();
		}
	);
	it('requires a selected file and shows a non-mutating preview with row feedback', async () => {
		render(CollectionImport);
		expect(screen.getByTestId('collection-import-preview')).toBeDisabled();
		const file = await upload();
		await screen.findByRole('heading', { name: 'Vorschau bereit' });
		expect(mocks.preview).toHaveBeenCalledWith(file, 'json');
		expect(mocks.commit).not.toHaveBeenCalled();
		expect(screen.getByTestId('import-counts')).toHaveTextContent('1 Konflikte, 1 ungültig');
		expect(screen.getByRole('region', { name: 'Import-Ergebnistabelle' })).toHaveAttribute(
			'tabindex',
			'0'
		);
		expect(screen.getByText('Vorhandene Daten bleiben erhalten.')).toBeInTheDocument();
		expect(
			screen.getByText(
				'Es wurden noch keine Sammlungseinträge verändert. Konflikte und ungültige Zeilen werden ausgelassen.'
			)
		).toBeInTheDocument();
	});
	it('requires a fresh preview after choosing another file or format', async () => {
		render(CollectionImport);
		await upload();
		await screen.findByTestId('collection-import-confirm');
		await fireEvent.change(screen.getByLabelText('Bestandsdatei'), {
			target: { files: [new File(['{}'], 'another.json')] }
		});
		expect(screen.queryByTestId('collection-import-confirm')).not.toBeInTheDocument();
		await fireEvent.click(screen.getByTestId('collection-import-preview'));
		await screen.findByTestId('collection-import-confirm');
		await fireEvent.change(screen.getByLabelText('Dateiformat'), { target: { value: 'csv' } });
		expect(screen.queryByTestId('collection-import-confirm')).not.toBeInTheDocument();
	});
	it('shows rows without a copy key and prevents submitting a cleared file selection', async () => {
		mocks.preview.mockResolvedValue(
			job({
				results: [
					{
						row_number: 1,
						copy_key: null,
						status: 'invalid',
						code: 'invalid_fields',
						entry_id: null
					}
				]
			})
		);
		render(CollectionImport);
		await upload();
		expect(await screen.findByRole('cell', { name: '–' })).toBeInTheDocument();
		const input = screen.getByLabelText<HTMLInputElement>('Bestandsdatei');
		await fireEvent.change(input, { target: { files: [] } });
		expect(screen.queryByTestId('import-result')).not.toBeInTheDocument();
		expect(screen.getByTestId('collection-import-preview')).toBeDisabled();
		await fireEvent.submit(input.form!);
		expect(mocks.preview).toHaveBeenCalledOnce();
	});
	it('confirms only explicitly and refreshes the offline snapshot after actual completion', async () => {
		render(CollectionImport);
		await upload();
		await fireEvent.click(await screen.findByTestId('collection-import-confirm'));
		await screen.findByRole('heading', { name: 'In Warteschlange' });
		expect(mocks.commit).toHaveBeenCalledWith('job-1');
		expect(mocks.refresh).not.toHaveBeenCalled();
		await fireEvent.click(screen.getByRole('button', { name: 'Status aktualisieren' }));
		await screen.findByRole('heading', { name: 'Import abgeschlossen' });
		await screen.findByText('Die Sammlung und der Offline-Bestand wurden aktualisiert.');
		expect(mocks.refresh).toHaveBeenCalledOnce();
	});
	it('keeps a completed server import successful when the local cache cannot refresh', async () => {
		mocks.commit.mockResolvedValue(job({ status: 'completed' }));
		mocks.refresh.mockResolvedValue(false);
		render(CollectionImport);
		await upload();
		await fireEvent.click(await screen.findByTestId('collection-import-confirm'));
		await screen.findByText(/Der Import ist auf dem Server abgeschlossen/);
		expect(screen.queryByTestId('transfer-error')).not.toBeInTheDocument();
	});
	it.each([
		[new Error('Bestätigung unterbrochen'), 'Bestätigung unterbrochen', 'Status unterbrochen'],
		[
			null,
			'Der Import konnte nicht gestartet werden.',
			'Der Auftragsstatus konnte nicht geladen werden.'
		]
	])(
		'preserves the preview and allows retry after a confirmation error of %s',
		async (cause, confirmationMessage, statusMessage) => {
			mocks.commit.mockRejectedValueOnce(cause);
			render(CollectionImport);
			await upload();
			await fireEvent.click(await screen.findByTestId('collection-import-confirm'));
			expect(await screen.findByRole('alert')).toHaveTextContent(confirmationMessage);
			expect(screen.getByTestId('collection-import-confirm')).not.toBeDisabled();
			mocks.fetchImport.mockRejectedValueOnce(
				cause instanceof Error ? new Error(statusMessage) : cause
			);
			await fireEvent.click(screen.getByRole('button', { name: 'Status aktualisieren' }));
			expect(await screen.findByRole('alert')).toHaveTextContent(statusMessage);
			await fireEvent.click(screen.getByRole('button', { name: 'Status aktualisieren' }));
			await screen.findByRole('heading', { name: 'Import abgeschlossen' });
			expect(screen.queryByRole('alert')).not.toBeInTheDocument();
		}
	);
	it('polls a queued import automatically and stops after completion', async () => {
		render(CollectionImport);
		await upload();
		const confirm = await screen.findByTestId('collection-import-confirm');
		vi.useFakeTimers();
		try {
			await fireEvent.click(confirm);
			expect(screen.getByRole('heading', { name: 'In Warteschlange' })).toBeInTheDocument();
			await vi.advanceTimersByTimeAsync(1_500);
			expect(screen.getByRole('heading', { name: 'Import abgeschlossen' })).toBeInTheDocument();
			expect(mocks.fetchImport).toHaveBeenCalledOnce();
			await vi.advanceTimersByTimeAsync(5_000);
			expect(mocks.fetchImport).toHaveBeenCalledOnce();
		} finally {
			vi.useRealTimers();
		}
	});
	it.each([
		[new Error('Ungültige CSV-Spalten'), 'Ungültige CSV-Spalten', 'Download unterbrochen'],
		[null, 'Die Datei konnte nicht geprüft werden.', 'Der Download ist fehlgeschlagen.']
	])(
		'allows retry after a failed upload of %s and reports a download error',
		async (cause, previewMessage, downloadMessage) => {
			mocks.preview.mockRejectedValueOnce(cause);
			render(CollectionImport);
			await upload();
			expect(await screen.findByRole('alert')).toHaveTextContent(previewMessage);
			expect(screen.queryByTestId('collection-import-confirm')).not.toBeInTheDocument();
			await fireEvent.click(screen.getByTestId('collection-import-preview'));
			await screen.findByRole('heading', { name: 'Vorschau bereit' });
			mocks.download.mockRejectedValueOnce(
				cause instanceof Error ? new Error(downloadMessage) : cause
			);
			await fireEvent.click(screen.getByTestId('collection-export-csv'));
			expect(await screen.findByRole('alert')).toHaveTextContent(downloadMessage);
		}
	);
	it.each(['success', 'failure'] as const)(
		'discards a preview after leaving (%s)',
		async (outcome) => {
			const response = Promise.withResolvers<TransferJob>();
			mocks.preview.mockReturnValueOnce(response.promise);
			const { unmount } = render(CollectionImport);
			await upload();
			expect(mocks.preview).toHaveBeenCalledOnce();
			expect(screen.getByLabelText('Bestandsdatei')).toBeDisabled();
			unmount();
			await settle(response, job(), outcome);
			expect(mocks.goto).not.toHaveBeenCalled();
		}
	);
	it.each(['success', 'failure'] as const)(
		'keeps import polling stopped if confirmation settles after leaving (%s)',
		async (outcome) => {
			const response = Promise.withResolvers<TransferJob>();
			mocks.commit.mockReturnValueOnce(response.promise);
			const { unmount } = render(CollectionImport);
			await upload();
			const confirm = await screen.findByTestId('collection-import-confirm');
			vi.useFakeTimers();
			try {
				await fireEvent.click(confirm);
				expect(mocks.commit).toHaveBeenCalledOnce();
				unmount();
				await settle(response, job({ status: 'queued' }), outcome);
				await vi.advanceTimersByTimeAsync(3_000);
				expect(mocks.fetchImport).not.toHaveBeenCalled();
			} finally {
				vi.useRealTimers();
			}
		}
	);
	it.each(['success', 'failure'] as const)(
		'keeps an import status refresh from updating private data after leaving (%s)',
		async (outcome) => {
			const response = Promise.withResolvers<TransferJob>();
			mocks.fetchImport.mockReturnValueOnce(response.promise);
			const { unmount } = render(CollectionImport);
			await upload();
			await screen.findByRole('heading', { name: 'Vorschau bereit' });
			await fireEvent.click(screen.getByRole('button', { name: 'Status aktualisieren' }));
			expect(mocks.fetchImport).toHaveBeenCalledWith('job-1', 1);
			unmount();
			await settle(response, job({ status: 'completed' }), outcome);
			expect(mocks.refresh).not.toHaveBeenCalled();
		}
	);
	it('downloads complete formats and templates using distinct actions', async () => {
		render(CollectionImport);
		for (const [name, format, template] of [
			['CSV herunterladen', 'csv', false],
			['JSON herunterladen', 'json', false],
			['CSV-Vorlage', 'csv', true],
			['JSON-Vorlage', 'json', true]
		] as const) {
			await fireEvent.click(screen.getByRole('button', { name }));
			await waitFor(() => expect(mocks.download).toHaveBeenLastCalledWith(format, template));
			await waitFor(() => expect(screen.getByRole('button', { name })).not.toBeDisabled());
		}
	});
	it('paginates results and preserves interrupted or expired job explanations', async () => {
		mocks.preview.mockResolvedValue(job({ total_rows: 101 }));
		mocks.fetchImport.mockResolvedValue(
			job({ total_rows: 101, page: 2, status: 'failed', results: [] })
		);
		render(CollectionImport);
		await upload();
		await screen.findByTestId('import-result');
		await fireEvent.click(screen.getByRole('button', { name: 'Weiter' }));
		await screen.findByText('Seite 2');
		expect(mocks.fetchImport).toHaveBeenCalledWith('job-1', 2);
		expect(screen.getByRole('button', { name: 'Weiter' })).toBeDisabled();
		expect(screen.getByRole('alert')).toHaveTextContent(
			'Bereits hinzugefügte Einträge bleiben erhalten'
		);
		await fireEvent.click(screen.getByRole('button', { name: 'Zurück' }));
		await waitFor(() => expect(mocks.fetchImport).toHaveBeenLastCalledWith('job-1', 1));
		mocks.fetchImport.mockResolvedValue(job({ status: 'expired' }));
		await fireEvent.click(screen.getByRole('button', { name: 'Status aktualisieren' }));
		await screen.findByText('Bitte prüfe die Datei erneut, um eine neue Vorschau zu erhalten.');
	});
	it('disables confirmation when there are no new records and stops after disposal', async () => {
		mocks.preview.mockResolvedValue(
			job({ counts: { new: 0, created: 0, unchanged: 3, conflict: 0, invalid: 0, failed: 0 } })
		);
		const rendered = render(CollectionImport);
		await upload();
		expect(await screen.findByTestId('collection-import-confirm')).toBeDisabled();
		rendered.unmount();
		expect(mocks.commit).not.toHaveBeenCalled();
	});
	it('blocks offline transfers and redirects an anonymous user', async () => {
		mocks.offline.online = false;
		render(CollectionImport);
		expect(screen.getByTestId('collection-export-csv')).toBeDisabled();
		expect(
			screen.getByText('Import und Export benötigen eine Internetverbindung.')
		).toBeInTheDocument();
		cleanup();
		mocks.auth.isAuthenticated = false;
		render(CollectionImport);
		await waitFor(() => expect(mocks.goto).toHaveBeenCalledWith('/login'));
	});
});

describe('personal data export page', () => {
	it.each(['success', 'failure'] as const)(
		'preserves a newer export list when an older request returns %s',
		async (outcome) => {
			const outdated = Promise.withResolvers<TransferJob[]>();
			mocks.list
				.mockResolvedValueOnce([])
				.mockReturnValueOnce(outdated.promise)
				.mockResolvedValueOnce([job({ kind: 'export', status: 'ready' })]);
			render(PersonalExport);
			await screen.findByText('Noch kein Export angefordert.');
			const refresh = screen.getByRole('button', { name: 'Aktualisieren' });
			await fireEvent.click(refresh);
			await fireEvent.click(refresh);
			await screen.findByTestId('download-personal-export');
			await settle(outdated, [job({ kind: 'export', status: 'queued' })], outcome);
			expect(screen.getByTestId('download-personal-export')).toBeEnabled();
			expect(screen.queryByRole('alert')).not.toBeInTheDocument();
		}
	);
	it.each(['success', 'failure'] as const)(
		'discards an export list response after leaving (%s)',
		async (outcome) => {
			const response = Promise.withResolvers<TransferJob[]>();
			mocks.list.mockReturnValueOnce(response.promise);
			vi.useFakeTimers();
			try {
				const { unmount } = render(PersonalExport);
				await act(() => undefined);
				expect(mocks.list).toHaveBeenCalledOnce();
				unmount();
				await settle(response, [job({ kind: 'export', status: 'queued' })], outcome);
				await vi.advanceTimersByTimeAsync(3_000);
				expect(mocks.list).toHaveBeenCalledOnce();
			} finally {
				vi.useRealTimers();
			}
		}
	);
	it('allows refreshing the export list after an unexpected load failure', async () => {
		mocks.list.mockRejectedValueOnce(null).mockResolvedValue([]);
		render(PersonalExport);
		expect(await screen.findByRole('alert')).toHaveTextContent(
			'Die Exportaufträge konnten nicht geladen werden.'
		);
		await fireEvent.click(screen.getByRole('button', { name: 'Aktualisieren' }));
		await screen.findByText('Noch kein Export angefordert.');
		expect(screen.queryByRole('alert')).not.toBeInTheDocument();
	});
	it('returns to confirmation when authentication expires during a request or download', async () => {
		mocks.request.mockRejectedValueOnce(
			new TransferError('Anmeldung erneut bestätigen', 'RECENT_AUTH_REQUIRED')
		);
		render(PersonalExport);
		await screen.findByText('Noch kein Export angefordert.');
		await fireEvent.click(screen.getByTestId('request-personal-export'));
		await screen.findByLabelText('Aktuelles Passwort');
		cleanup();
		mocks.list.mockResolvedValue([job({ kind: 'export', status: 'ready' })]);
		mocks.personalDownload.mockRejectedValueOnce(
			new TransferError('Anmeldung erneut bestätigen', 'RECENT_AUTH_REQUIRED')
		);
		render(PersonalExport);
		await fireEvent.click(await screen.findByTestId('download-personal-export'));
		await screen.findByLabelText('Aktuelles Passwort');
	});
	it('starts Google confirmation and stores only a fixed return marker', async () => {
		mocks.options.mockResolvedValue({
			recent_authentication: false,
			password: false,
			google: true,
			github: false
		});
		mocks.oauth.mockResolvedValue('#export-reauth');
		render(PersonalExport);
		await fireEvent.click(await screen.findByRole('button', { name: 'Mit Google bestätigen' }));
		await waitFor(() => expect(window.location.hash).toBe('#export-reauth'));
		expect(sessionStorage.getItem('lilly-data-export-reauth')).toBe('pending');
		expect(mocks.oauth).toHaveBeenCalledWith('google', 'reauth');
	});
	it('paginates older jobs and returns to the newest page', async () => {
		mocks.list
			.mockResolvedValueOnce(
				Array.from({ length: 20 }, (_, index) =>
					job({ id: `job-${index}`, kind: 'export', status: 'expired' })
				)
			)
			.mockResolvedValueOnce([job({ kind: 'export', status: 'expired' })])
			.mockResolvedValue([]);
		render(PersonalExport);
		await screen.findAllByTestId('personal-export-job');
		await fireEvent.click(screen.getByRole('button', { name: 'Ältere Aufträge' }));
		await screen.findByText('Seite 2');
		expect(mocks.list).toHaveBeenLastCalledWith(2);
		await fireEvent.click(screen.getByRole('button', { name: 'Neuere Aufträge' }));
		await screen.findByText('Seite 1');
		expect(mocks.list).toHaveBeenLastCalledWith(1);
	});
	it('redirects an anonymous user without requesting private job metadata', async () => {
		mocks.auth.isAuthenticated = false;
		render(PersonalExport);
		await waitFor(() => expect(mocks.goto).toHaveBeenCalledWith('/login'));
		expect(mocks.list).not.toHaveBeenCalled();
	});
	it('requests an export, shows the exact expiry and starts the protected download', async () => {
		const ready = job({ kind: 'export', status: 'ready' });
		mocks.list.mockResolvedValueOnce([]).mockResolvedValue([ready]);
		render(PersonalExport);
		await screen.findByText('Noch kein Export angefordert.');
		await fireEvent.click(screen.getByTestId('request-personal-export'));
		const download = await screen.findByTestId('download-personal-export');
		expect(mocks.request).toHaveBeenCalledOnce();
		expect(screen.getByText(/Download verfügbar bis/)).toBeInTheDocument();
		await waitFor(() => expect(download).not.toBeDisabled());
		await fireEvent.click(download);
		await waitFor(() => expect(mocks.personalDownload).toHaveBeenCalledWith('job-1'));
	});
	it('requires password reauthentication, clears the password and enables generation afterward', async () => {
		mocks.options
			.mockResolvedValueOnce({ recent_authentication: false, password: true })
			.mockResolvedValue({ recent_authentication: true, password: true });
		render(PersonalExport);
		const password = await screen.findByLabelText('Aktuelles Passwort');
		expect(screen.getByTestId('request-personal-export')).toBeDisabled();
		await fireEvent.input(password, { target: { value: 'temporary-password' } });
		await fireEvent.click(screen.getByTestId('export-confirm-password'));
		await screen.findByText('Deine Anmeldung ist bestätigt.');
		expect(mocks.password).toHaveBeenCalledWith('temporary-password');
		expect(screen.queryByLabelText('Aktuelles Passwort')).not.toBeInTheDocument();
		expect(screen.getByTestId('request-personal-export')).not.toBeDisabled();
	});
	it.each([
		[new Error('Passwort ist falsch'), 'Passwort ist falsch'],
		[null, 'Die Bestätigung ist fehlgeschlagen.']
	])(
		'clears the password and allows another attempt after a failure of %s',
		async (cause, message) => {
			mocks.options.mockResolvedValue({ recent_authentication: false, password: true });
			mocks.password.mockRejectedValue(cause);
			render(PersonalExport);
			const password = await screen.findByLabelText('Aktuelles Passwort');
			await fireEvent.input(password, { target: { value: 'wrong-password' } });
			await fireEvent.click(screen.getByTestId('export-confirm-password'));
			expect(await screen.findByRole('alert')).toHaveTextContent(message);
			expect(password).toHaveValue('');
			await fireEvent.input(password, { target: { value: 'another-password' } });
			expect(screen.getByTestId('export-confirm-password')).toBeEnabled();
		}
	);
	it.each(['success', 'failure'] as const)(
		'does not reload private jobs after leaving during password confirmation (%s)',
		async (outcome) => {
			const response = Promise.withResolvers<void>();
			mocks.options.mockResolvedValue({ recent_authentication: false, password: true });
			mocks.password.mockReturnValueOnce(response.promise);
			const { unmount } = render(PersonalExport);
			const password = await screen.findByLabelText('Aktuelles Passwort');
			await fireEvent.input(password, { target: { value: 'temporary-password' } });
			await fireEvent.click(screen.getByTestId('export-confirm-password'));
			expect(mocks.password).toHaveBeenCalledOnce();
			unmount();
			await settle(response, undefined, outcome);
			expect(mocks.list).toHaveBeenCalledOnce();
		}
	);
	it.each([
		[new Error('Provider nicht verfügbar'), 'Provider nicht verfügbar'],
		[null, 'Die Anmeldung konnte nicht bestätigt werden.']
	])('offers only the linked OAuth provider and allows retry after %s', async (cause, message) => {
		mocks.options.mockResolvedValue({
			recent_authentication: false,
			password: false,
			google: false,
			github: true
		});
		mocks.oauth.mockRejectedValue(cause);
		render(PersonalExport);
		const confirm = await screen.findByRole('button', { name: 'Mit GitHub bestätigen' });
		expect(screen.queryByLabelText('Aktuelles Passwort')).not.toBeInTheDocument();
		expect(screen.queryByRole('button', { name: 'Mit Google bestätigen' })).not.toBeInTheDocument();
		await fireEvent.click(confirm);
		expect(await screen.findByRole('alert')).toHaveTextContent(message);
		expect(confirm).toBeEnabled();
		expect(mocks.oauth).toHaveBeenCalledWith('github', 'reauth');
	});
	it.each(['success', 'failure'] as const)(
		'keeps the next page open when OAuth settles after leaving (%s)',
		async (outcome) => {
			const response = Promise.withResolvers<string>();
			mocks.options.mockResolvedValue({ recent_authentication: false, github: true });
			mocks.oauth.mockReturnValueOnce(response.promise);
			const { unmount } = render(PersonalExport);
			await fireEvent.click(await screen.findByRole('button', { name: 'Mit GitHub bestätigen' }));
			expect(mocks.oauth).toHaveBeenCalledOnce();
			unmount();
			await settle(response, '#late-confirmation', outcome);
			expect(window.location.hash).toBe('');
			expect(sessionStorage.getItem('lilly-data-export-reauth')).toBeNull();
		}
	);
	it('explains failed, cancelled and expired exports without offering old downloads', async () => {
		mocks.list.mockResolvedValue([
			job({ id: 'failed', kind: 'export', status: 'failed' }),
			job({ id: 'cancelled', kind: 'export', status: 'cancelled' }),
			job({ id: 'expired', kind: 'export', status: 'expired' })
		]);
		render(PersonalExport);
		await screen.findByText(/Der Export konnte nicht vollständig erstellt werden/);
		expect(screen.getByText(/durch eine Änderung oder Löschung/)).toBeInTheDocument();
		expect(screen.getByText(/Downloadfrist ist abgelaufen/)).toBeInTheDocument();
		expect(screen.queryByTestId('download-personal-export')).not.toBeInTheDocument();
	});
	it('does not duplicate running exports and keeps status refresh available after a failure', async () => {
		mocks.list.mockResolvedValue([job({ kind: 'export', status: 'running' })]);
		render(PersonalExport);
		await screen.findByText('Wird verarbeitet');
		expect(screen.getByTestId('request-personal-export')).toBeDisabled();
		mocks.list.mockRejectedValueOnce(new Error('Status nicht erreichbar'));
		await fireEvent.click(screen.getByRole('button', { name: 'Aktualisieren' }));
		await screen.findByText('Status nicht erreichbar');
	});
	it.each([
		[new Error('Limit erreicht'), 'Limit erreicht'],
		[null, 'Der Export konnte nicht angefordert werden.']
	])('allows another export request after a failure of %s', async (cause, message) => {
		mocks.request.mockRejectedValueOnce(cause);
		render(PersonalExport);
		await screen.findByText('Noch kein Export angefordert.');
		const request = screen.getByTestId('request-personal-export');
		await fireEvent.click(request);
		expect(await screen.findByRole('alert')).toHaveTextContent(message);
		expect(request).toBeEnabled();
	});
	it.each(['success', 'failure'] as const)(
		'does not request a new list if export creation settles after leaving (%s)',
		async (outcome) => {
			const response = Promise.withResolvers<TransferJob>();
			mocks.request.mockReturnValueOnce(response.promise);
			const { unmount } = render(PersonalExport);
			await screen.findByText('Noch kein Export angefordert.');
			await fireEvent.click(screen.getByTestId('request-personal-export'));
			expect(mocks.request).toHaveBeenCalledOnce();
			unmount();
			await settle(response, job({ kind: 'export', status: 'queued' }), outcome);
			expect(mocks.list).toHaveBeenCalledOnce();
		}
	);
	it.each([
		[new Error('Die Exportdatei ist abgelaufen.'), 'Die Exportdatei ist abgelaufen.'],
		[null, 'Der Download konnte nicht gestartet werden.']
	])('keeps the list visible after a download failure of %s', async (cause, message) => {
		mocks.list.mockResolvedValue([job({ kind: 'export', status: 'ready' })]);
		mocks.personalDownload.mockRejectedValueOnce(cause);
		render(PersonalExport);
		const download = await screen.findByTestId('download-personal-export');
		await fireEvent.click(download);
		expect(await screen.findByRole('alert')).toHaveTextContent(message);
		expect(download).toBeEnabled();
	});
	it('blocks creation and download while offline', async () => {
		mocks.offline.online = false;
		mocks.list.mockResolvedValue([job({ kind: 'export', status: 'ready' })]);
		render(PersonalExport);
		await screen.findByText('Die Datenauskunft benötigt eine Internetverbindung.');
		expect(screen.getByTestId('request-personal-export')).toBeDisabled();
		expect(await screen.findByTestId('download-personal-export')).toBeDisabled();
	});
});
