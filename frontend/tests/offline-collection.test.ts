import 'fake-indexeddb/auto';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import {
	listConflicts,
	listMutations,
	filterLocalCollection,
	queueCollectionCreate,
	queueCollectionUpdate,
	readLocalCollection,
	reapplyConflictLocalVersion,
	acceptConflictServerVersion,
	synchronizeCollection
} from '$lib/offline/collection';
import {
	deleteStoredCollectionEntry,
	listStoredCollectionEntries,
	putCollectionEntry,
	replaceOfflineSnapshot,
	resetOfflineDatabaseForTests
} from '$lib/offline/database';
import type { CollectionSyncResponse } from '$lib/offline/types';
import { entry, issue, snapshot } from './fixtures/offline';

describe('offline collection queue', () => {
	beforeEach(async () => {
		await resetOfflineDatabaseForTests();
		await replaceOfflineSnapshot(snapshot(1));
	});

	it('filters cached entries with the same pagination semantics', async () => {
		await replaceOfflineSnapshot(
			snapshot(1, [
				entry,
				{ ...entry, id: 31, issue_id: 21, issue_number: 2, title: 'Nacht über Berlin' }
			])
		);
		const result = await readLocalCollection(1, { q: 'Gott', page: 1, per_page: 1 });

		expect(result.total).toBe(1);
		expect(result.data[0].id).toBe(30);
		expect(result.per_page).toBe(1);
	});

	it('supports every local filter, sort order and pagination boundary', () => {
		const entries = [
			entry,
			{
				...entry,
				id: 31,
				issue_id: 21,
				issue_number: 2,
				title: 'Alpha',
				series_id: 11,
				series_name: 'John Sinclair',
				series_slug: 'john-sinclair',
				condition_grade: 'Z1' as const,
				status: 'duplicate' as const,
				created_at: '2026-08-16T01:00:00Z'
			},
			{
				...entry,
				id: 32,
				issue_id: 22,
				title: 'Zukunft',
				condition_grade: null,
				created_at: null
			},
			{ ...entry, id: 33 }
		];
		const authors = new Map([
			[20, ['Jo Zybell']],
			[21, ['Jason Dark']]
		]);

		expect(
			filterLocalCollection(entries, { sort: 'issue_number', sort_dir: 'desc' }).data[0].id
		).toBe(31);
		expect(filterLocalCollection(entries, { sort: 'condition' }).data.at(-1)?.id).toBe(32);
		expect(filterLocalCollection(entries, { sort: 'title' }).data[0].title).toBe('Alpha');
		expect(filterLocalCollection(entries, { sort: 'added', sort_dir: 'desc' }).data[0].id).toBe(31);
		expect(filterLocalCollection(entries, {}).data.map((item) => item.id)).toContain(33);

		const exclusions = [
			{ series_slug: 'unknown' },
			{ issue_id: 999 },
			{ status: 'wanted' as const },
			{ issue_number: 999 },
			{ condition: 'Z4' as const },
			{ condition_min: 'Z3' as const },
			{ condition_max: 'Z0' as const },
			{ title: 'nicht vorhanden' },
			{ author: 'nicht vorhanden' },
			{ q: 'nicht vorhanden' }
		];
		for (const params of exclusions) {
			expect(filterLocalCollection([entry], params, authors).total).toBe(0);
		}
		expect(filterLocalCollection(entries, { author: 'dark' }, authors).data[0].id).toBe(31);
		expect(filterLocalCollection(entries, { q: 'zybell' }, authors).total).toBe(2);
		expect(filterLocalCollection(entries, { q: 'alpha' }, authors).total).toBe(1);
		expect(filterLocalCollection(entries, { page: 0, per_page: 200 })).toMatchObject({
			page: 1,
			per_page: 100
		});
		expect(filterLocalCollection(entries, { per_page: 0 }).per_page).toBe(1);
	});

	it('derives missing issues from the cached reference data', async () => {
		await replaceOfflineSnapshot({
			...snapshot(1),
			issues: [
				issue,
				{
					...issue,
					id: 21,
					issue_number: 2,
					title: 'Die fehlende Ausgabe',
					authors: ['Ian Rolf Hill']
				},
				{ ...issue, id: 22, series_id: 999, issue_number: 3, title: 'Unbekannte Serie' }
			]
		});

		const result = await readLocalCollection(1, { status: 'missing', author: 'rolf' });

		expect(result.data).toHaveLength(1);
		expect(result.data[0]).toMatchObject({
			id: 0,
			issue_id: 21,
			series_name: 'Maddrax',
			status: 'missing',
			revision: null
		});
		const unknownSeries = await readLocalCollection(1, {
			status: 'missing',
			issue_id: 22
		});
		expect(unknownSeries.data[0]).toMatchObject({ series_name: '', series_slug: '' });
	});

	it('rejects creates when cached issue or series reference data is missing', async () => {
		await expect(queueCollectionCreate(1, { issue_id: 999 })).rejects.toThrow(
			'Dieses Heft ist offline noch nicht verfügbar.'
		);
		await replaceOfflineSnapshot({ ...snapshot(1), series: [], issues: [issue] });
		await expect(queueCollectionCreate(1, { issue_id: issue.id })).rejects.toThrow(
			'Die Serie ist offline noch nicht verfügbar.'
		);
	});

	it('keeps pending creates from separate module instances under distinct temporary IDs', async () => {
		const first = await queueCollectionCreate(1, {
			issue_id: issue.id,
			notes: 'first session'
		});
		const laterTimestamp = Math.abs(first.entry.id) + 1_000;
		const now = vi.spyOn(Date, 'now').mockReturnValue(laterTimestamp);
		vi.resetModules();

		try {
			const { queueCollectionCreate: queueAfterReload } = await import('$lib/offline/collection');
			const second = await queueAfterReload(1, {
				issue_id: issue.id,
				notes: 'second session'
			});

			expect(second.entry.id).not.toBe(first.entry.id);
			const pendingEntries = (await listStoredCollectionEntries(1)).filter(({ id }) => id < 0);
			expect(pendingEntries).toHaveLength(2);
			expect(pendingEntries.map(({ notes }) => notes)).toEqual(
				expect.arrayContaining(['first session', 'second session'])
			);
			expect(await listMutations(1)).toHaveLength(2);
		} finally {
			now.mockRestore();
		}
	});

	it('falls back to a generated UUID when randomUUID is unavailable', async () => {
		vi.stubGlobal('crypto', {});
		vi.spyOn(Math, 'random').mockReturnValue(0.5);

		const created = await queueCollectionCreate(1, { issue_id: issue.id });

		expect(created.mutation.mutation_id).toMatch(/^[0-9a-f-]{36}$/);
		vi.unstubAllGlobals();
		vi.restoreAllMocks();
	});

	it('merges edits into a pending create and syncs it exactly once', async () => {
		const created = await queueCollectionCreate(1, {
			issue_id: 20,
			condition_grade: 'Z2',
			status: 'owned'
		});
		await queueCollectionUpdate(1, created.entry.id, {
			condition_grade: 'Z1',
			notes: 'offline geändert'
		});

		const pending = await listMutations(1);
		expect(pending).toHaveLength(1);
		expect(pending[0].operation).toBe('create');
		if (pending[0].operation === 'create') {
			expect(pending[0].entry.condition_grade).toBe('Z1');
			expect(pending[0].entry.notes).toBe('offline geändert');
		}

		const serverEntry = { ...entry, id: 99, revision: 1, condition_grade: 'Z1' as const };
		const send = vi.fn(async (): Promise<CollectionSyncResponse> => ({
			results: [
				{
					mutation_id: pending[0].mutation_id,
					status: 'applied',
					entry: serverEntry,
					error: null,
					code: null
				}
			]
		}));
		await synchronizeCollection(1, send);

		expect(send).toHaveBeenCalledOnce();
		expect(await listMutations(1)).toEqual([]);
		expect(
			(await listStoredCollectionEntries(1)).some((item) => item.id === created.entry.id)
		).toBe(false);
		expect((await listStoredCollectionEntries(1)).find((item) => item.id === 99)?.sync_state).toBe(
			'synced'
		);
	});

	it('merges repeated edits into one pending update', async () => {
		await queueCollectionUpdate(1, entry.id, { notes: 'erste Änderung' });
		const second = await queueCollectionUpdate(1, entry.id, {
			condition_grade: 'Z0',
			edition_label: '  Erstauflage  '
		});

		expect(await listMutations(1)).toHaveLength(1);
		expect(second.mutation).toMatchObject({
			operation: 'update',
			changes: {
				notes: 'erste Änderung',
				condition_grade: 'Z0',
				edition_label: '  Erstauflage  '
			}
		});
		expect(second.entry).toMatchObject({ edition_label: 'Erstauflage', sync_state: 'pending' });
	});

	it('rejects updates for missing entries and orphaned temporary entries', async () => {
		await expect(queueCollectionUpdate(1, 999, { notes: 'x' })).rejects.toThrow(
			'Der Sammlungseintrag ist offline noch nicht verfügbar.'
		);
		await putCollectionEntry(1, { ...entry, id: -999, sync_state: 'pending' });
		await expect(queueCollectionUpdate(1, -999, { notes: 'x' })).rejects.toThrow(
			'Die ausstehende Offline-Änderung wurde nicht gefunden.'
		);
	});

	it('parks a stale update until the user chooses a resolution', async () => {
		const queued = await queueCollectionUpdate(1, entry.id, { condition_grade: 'Z1' });
		const serverEntry = { ...entry, revision: 2, condition_grade: 'Z3' as const };
		await synchronizeCollection(1, async () => ({
			results: [
				{
					mutation_id: queued.mutation.mutation_id,
					status: 'conflict',
					entry: serverEntry,
					error: 'Serverstand ist neuer',
					code: 'collection_revision_conflict'
				}
			]
		}));

		const [conflict] = await listConflicts(1);
		expect(conflict.server_entry?.revision).toBe(2);
		expect((await listStoredCollectionEntries(1))[0].sync_state).toBe('conflict');
		expect(await listMutations(1)).toEqual([]);

		const reapplied = await reapplyConflictLocalVersion(conflict);
		expect(reapplied.operation).toBe('update');
		if (reapplied.operation === 'update') expect(reapplied.base_revision).toBe(2);
		expect(await listConflicts(1)).toEqual([]);
	});

	it('can explicitly accept the server version', async () => {
		const queued = await queueCollectionUpdate(1, entry.id, { condition_grade: 'Z1' });
		const serverEntry = { ...entry, revision: 2, condition_grade: 'Z4' as const };
		await synchronizeCollection(1, async () => ({
			results: [
				{
					mutation_id: queued.mutation.mutation_id,
					status: 'conflict',
					entry: serverEntry,
					error: 'Konflikt',
					code: 'collection_revision_conflict'
				}
			]
		}));
		const [conflict] = await listConflicts(1);
		await acceptConflictServerVersion(conflict);

		expect(await listConflicts(1)).toEqual([]);
		expect((await listStoredCollectionEntries(1))[0]).toMatchObject({
			condition_grade: 'Z4',
			revision: 2,
			sync_state: 'synced'
		});
	});

	it('keeps transient failures queued and counts attempts', async () => {
		await queueCollectionUpdate(1, entry.id, { notes: 'bleibt lokal' });
		await expect(
			synchronizeCollection(1, async () => {
				throw new TypeError('Network unavailable');
			})
		).rejects.toThrow('Network unavailable');

		const [pending] = await listMutations(1);
		expect(pending.attempts).toBe(1);
		expect(pending.last_error).toBe('Network unavailable');
	});

	it('uses a generic message for non-Error synchronization failures', async () => {
		await queueCollectionUpdate(1, entry.id, { notes: 'bleibt lokal' });
		await expect(synchronizeCollection(1, async () => Promise.reject('offline'))).rejects.toBe(
			'offline'
		);

		expect((await listMutations(1))[0].last_error).toBe('Synchronisierung fehlgeschlagen');
	});

	it('returns null without queued work and coalesces concurrent synchronization attempts', async () => {
		const unusedSend = vi.fn();
		await expect(synchronizeCollection(1, unusedSend)).resolves.toBeNull();
		expect(unusedSend).not.toHaveBeenCalled();

		await queueCollectionUpdate(1, entry.id, { notes: 'einmal senden' });
		let finish: ((response: CollectionSyncResponse) => void) | undefined;
		const send = vi.fn(
			() =>
				new Promise<CollectionSyncResponse>((resolve) => {
					finish = resolve;
				})
		);
		const first = synchronizeCollection(1, send);
		const second = synchronizeCollection(1, send);
		await vi.waitFor(() => expect(send).toHaveBeenCalledOnce());
		const [mutation] = await listMutations(1);
		finish?.({
			results: [
				{
					mutation_id: mutation.mutation_id,
					status: 'already_applied',
					entry: null,
					error: null,
					code: null
				}
			]
		});

		await expect(Promise.all([first, second])).resolves.toHaveLength(2);
		expect(send).toHaveBeenCalledOnce();
		expect(await listMutations(1)).toEqual([]);
	});

	it('ignores unknown results and records rejected mutations with default details', async () => {
		const queued = await queueCollectionUpdate(1, entry.id, { notes: 'Konflikt' });
		await deleteStoredCollectionEntry(1, entry.id);
		await synchronizeCollection(1, async () => ({
			results: [
				{
					mutation_id: 'unknown',
					status: 'applied',
					entry: null,
					error: null,
					code: null
				},
				{
					mutation_id: queued.mutation.mutation_id,
					status: 'rejected',
					entry: null,
					error: null,
					code: null
				}
			]
		}));

		expect(await listMutations(1)).toEqual([]);
		expect((await listConflicts(1))[0]).toMatchObject({
			error: 'Die Änderung konnte nicht synchronisiert werden.',
			status: 'rejected'
		});
	});

	it('deletes an unsaved create when accepting a conflict and rejects reapplying it', async () => {
		const created = await queueCollectionCreate(1, { issue_id: issue.id });
		await synchronizeCollection(1, async () => ({
			results: [
				{
					mutation_id: created.mutation.mutation_id,
					status: 'rejected',
					entry: null,
					error: 'duplicate',
					code: 'duplicate'
				}
			]
		}));
		const [conflict] = await listConflicts(1);

		await expect(reapplyConflictLocalVersion(conflict)).rejects.toThrow(
			'Diese Änderung kann nicht erneut angewendet werden.'
		);
		await acceptConflictServerVersion(conflict);

		expect((await listStoredCollectionEntries(1)).some(({ id }) => id === created.entry.id)).toBe(
			false
		);
		expect(await listConflicts(1)).toEqual([]);
	});
});
