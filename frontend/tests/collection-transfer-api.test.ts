import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
	commitCollectionImport,
	downloadCollection,
	downloadPersonalExport,
	fetchCollectionImport,
	fetchPersonalExport,
	fetchReauthenticationOptions,
	listPersonalExports,
	prepareCollectionTransfer,
	previewCollectionImport,
	refreshImportedCollection,
	requestPersonalExport,
	transferRowReason
} from '#lib/api/collection-transfer.js';

const mocks = vi.hoisted(() => ({
	me: vi.fn(),
	sync: vi.fn(),
	refresh: vi.fn(),
	mutations: vi.fn(),
	conflicts: vi.fn(),
	version: vi.fn(),
	current: vi.fn()
}));
vi.mock('#lib/api/auth.js', () => ({ fetchMe: mocks.me }));
vi.mock('#lib/api/collection.js', () => ({
	syncPendingCollectionChanges: mocks.sync,
	refreshOfflineSnapshot: mocks.refresh
}));
vi.mock('#lib/offline/database.js', () => ({
	listMutations: mocks.mutations,
	listConflicts: mocks.conflicts
}));
vi.mock('#lib/offline/session.js', () => ({
	offlineSessionVersion: mocks.version,
	isCurrentOfflineSession: mocks.current
}));

const fetchMock = vi.fn();
const job = { id: 'job-1', status: 'ready', expires_at: '2099-01-01T00:00:00Z' };
const response = (data: unknown, status = 200) =>
	new Response(JSON.stringify(data), { status, headers: { 'Content-Type': 'application/json' } });

beforeEach(() => {
	vi.resetAllMocks();
	vi.stubGlobal('fetch', fetchMock);
	Object.defineProperty(navigator, 'onLine', { configurable: true, value: true });
	mocks.me.mockResolvedValue({ id: 7 });
	mocks.sync.mockResolvedValue(null);
	mocks.mutations.mockResolvedValue([]);
	mocks.conflicts.mockResolvedValue([]);
	mocks.version.mockReturnValue('session-1');
	mocks.current.mockReturnValue(true);
	mocks.refresh.mockResolvedValue(undefined);
});
afterEach(() => {
	vi.restoreAllMocks();
	vi.unstubAllGlobals();
	vi.useRealTimers();
});

describe('collection transfer account and offline boundaries', () => {
	it('flushes pending writes and checks conflicts for the authenticated account', async () => {
		await prepareCollectionTransfer();
		expect(mocks.sync).toHaveBeenCalledOnce();
		expect(mocks.mutations).toHaveBeenCalledWith(7);
		expect(mocks.conflicts).toHaveBeenCalledWith(7);
	});
	it.each(['mutations', 'conflicts'] as const)(
		'blocks a transfer while %s remain without deleting the queue',
		async (key) => {
			mocks[key].mockResolvedValue([{}]);
			await expect(prepareCollectionTransfer()).rejects.toThrow('Offline-Konflikte');
			expect(fetchMock).not.toHaveBeenCalled();
		}
	);
	it('rejects offline and changed sessions before uploading private files', async () => {
		Object.defineProperty(navigator, 'onLine', { configurable: true, value: false });
		await expect(prepareCollectionTransfer()).rejects.toThrow('Internetverbindung');
		expect(mocks.me).not.toHaveBeenCalled();
		Object.defineProperty(navigator, 'onLine', { configurable: true, value: true });
		mocks.current.mockReturnValue(false);
		await expect(prepareCollectionTransfer()).rejects.toThrow('Sitzung');
	});
	it('uploads one FormData file and sends no client-selected owner', async () => {
		fetchMock.mockResolvedValue(response(job));
		const file = new File(['{}'], 'collection.json', { type: 'application/json' });
		expect(await previewCollectionImport(file, 'json')).toEqual(job);
		expect(fetchMock).toHaveBeenCalledWith(
			'/api/v1/me/collection/imports/preview?format=json',
			expect.objectContaining({ method: 'POST', credentials: 'same-origin', cache: 'no-store' })
		);
		const init = fetchMock.mock.calls[0][1] as RequestInit;
		expect([...(init.body as FormData).keys()]).toEqual(['file']);
		expect((init.body as FormData).get('file')).toEqual(file);
	});
	it('confirms only an encoded own job and paginates results', async () => {
		fetchMock.mockImplementation(async () => response(job));
		await commitCollectionImport('job/1');
		await fetchCollectionImport('job/1', 2);
		expect(fetchMock).toHaveBeenNthCalledWith(
			1,
			'/api/v1/me/collection/imports/job%2F1/commit',
			expect.objectContaining({ method: 'POST' })
		);
		expect(fetchMock).toHaveBeenNthCalledWith(
			2,
			'/api/v1/me/collection/imports/job%2F1?page=2',
			expect.anything()
		);
	});
	it('keeps server success distinct from a failed offline snapshot refresh', async () => {
		expect(await refreshImportedCollection()).toBe(true);
		mocks.refresh.mockRejectedValue(new Error('storage unavailable'));
		expect(await refreshImportedCollection()).toBe(false);
	});
});

describe('private requests and readable errors', () => {
	it.each([
		[401, undefined, 'Sitzung'],
		[403, undefined, 'Anmeldung'],
		[404, undefined, 'nicht verfügbar'],
		[409, undefined, 'aktualisiere'],
		[413, undefined, 'Größe'],
		[429, undefined, 'Zu viele'],
		[400, 'invalid_csv_header', 'CSV-Spalten'],
		[400, 'unsupported_format_version', 'Formatversion'],
		[409, 'preview_expired', 'Vorschau'],
		[409, 'transfer_active', 'bereits'],
		[429, 'transfer_limit', 'tägliche'],
		[409, 'export_invalidated', 'Kontodaten'],
		[500, undefined, 'fehlgeschlagen']
	])('handles HTTP %s / %s without displaying server internals', async (status, code, message) => {
		fetchMock.mockResolvedValue(
			response({ code, error: 'PRIVATE SERVER DETAIL' }, status as number)
		);
		await expect(fetchPersonalExport('job')).rejects.toThrow(message as string);
	});
	it('handles malformed error bodies and account switches during response reading', async () => {
		fetchMock.mockResolvedValue(new Response('upstream failure', { status: 502 }));
		await expect(listPersonalExports()).rejects.toThrow('fehlgeschlagen');
		fetchMock.mockResolvedValue(response([]));
		mocks.current.mockReturnValue(false);
		await expect(listPersonalExports()).rejects.toThrow('Sitzung');
	});
	it('uses protected no-store requests for options, status, list and generation', async () => {
		fetchMock.mockImplementation(async () => response(job));
		await fetchReauthenticationOptions();
		await listPersonalExports();
		await requestPersonalExport();
		await fetchPersonalExport('a/b');
		expect(fetchMock.mock.calls.map((call) => call[0])).toEqual([
			'/api/v1/me/reauthentication/options',
			'/api/v1/me/data-exports',
			'/api/v1/me/data-exports',
			'/api/v1/me/data-exports/a%2Fb'
		]);
		expect(
			fetchMock.mock.calls.every(
				(call) => call[1].cache === 'no-store' && call[1].credentials === 'same-origin'
			)
		).toBe(true);
		expect(fetchMock.mock.calls[2][1].method).toBe('POST');
	});
});

describe('downloads', () => {
	it('does not start a native download after the session changes between preflight checks', async () => {
		const click = vi
			.spyOn(HTMLAnchorElement.prototype, 'click')
			.mockImplementation(() => undefined);
		fetchMock
			.mockResolvedValueOnce(response({ recent_authentication: true }))
			.mockResolvedValueOnce(response(job));
		mocks.current.mockReturnValueOnce(true).mockReturnValueOnce(true).mockReturnValueOnce(false);
		await expect(downloadPersonalExport('job-1')).rejects.toThrow('Sitzung');
		expect(click).not.toHaveBeenCalled();
	});
	it('downloads CSV or a template and revokes temporary object URLs', async () => {
		vi.useFakeTimers();
		const click = vi
			.spyOn(HTMLAnchorElement.prototype, 'click')
			.mockImplementation(() => undefined);
		const revoke = vi.fn();
		const create = vi.fn().mockReturnValue('blob:private-download');
		vi.stubGlobal(
			'URL',
			class extends URL {
				static createObjectURL = create;
				static revokeObjectURL = revoke;
			}
		);
		fetchMock.mockImplementation(async () => new Response('collection bytes'));
		await downloadCollection('csv');
		await downloadCollection('json', true);
		expect(mocks.sync).toHaveBeenCalledTimes(1);
		expect(click).toHaveBeenCalledTimes(2);
		expect(fetchMock.mock.calls.map((call) => call[0])).toEqual([
			'/api/v1/me/collection/export?format=csv',
			'/api/v1/me/collection/import/template?format=json'
		]);
		expect(document.querySelector('a[download]')).toBeNull();
		await vi.runAllTimersAsync();
		expect(revoke).toHaveBeenCalledTimes(2);
	});
	it('prevents private blob delivery after the session changes', async () => {
		const click = vi
			.spyOn(HTMLAnchorElement.prototype, 'click')
			.mockImplementation(() => undefined);
		fetchMock.mockResolvedValue(new Response('private'));
		mocks.current.mockReturnValue(false);
		await expect(downloadCollection('csv', true)).rejects.toThrow('Sitzung');
		expect(click).not.toHaveBeenCalled();
	});
	it('streams personal archives through the native download after fresh checks', async () => {
		const click = vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(function (
			this: HTMLAnchorElement
		) {
			expect(this.getAttribute('href')).toBe('/api/v1/me/data-exports/job-1/download');
		});
		fetchMock
			.mockResolvedValueOnce(response({ recent_authentication: true }))
			.mockResolvedValueOnce(response(job));
		await downloadPersonalExport('job-1');
		expect(click).toHaveBeenCalledOnce();
	});
	it('requires recent authentication and rejects expired archives before native download', async () => {
		fetchMock.mockResolvedValueOnce(response({ recent_authentication: false }));
		await expect(downloadPersonalExport('job-1')).rejects.toMatchObject({
			code: 'RECENT_AUTH_REQUIRED'
		});
		fetchMock
			.mockResolvedValueOnce(response({ recent_authentication: true }))
			.mockResolvedValueOnce(response({ ...job, expires_at: '2000-01-01T00:00:00Z' }));
		await expect(downloadPersonalExport('job-1')).rejects.toMatchObject({
			code: 'export_unavailable'
		});
	});
});

describe('row feedback', () => {
	it('distinguishes removal, changed data, duplicate keys and unknown records', () => {
		expect(transferRowReason('entry_removed')).toContain('abgegeben');
		expect(transferRowReason('entry_changed')).toContain('erhalten');
		expect(transferRowReason('duplicate_key_conflict')).toContain('widersprüchlichen');
		expect(transferRowReason('issue_not_found')).toContain('Katalog');
		expect(transferRowReason('future_code')).toContain('nicht übernommen');
		expect(transferRowReason(null)).toBe('');
	});
});
