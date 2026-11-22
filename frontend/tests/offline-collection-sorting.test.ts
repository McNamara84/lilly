import 'fake-indexeddb/auto';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { fetchCollection, type CollectionEntry } from '#lib/api/collection.js';
import { filterLocalCollection, readLocalCollection } from '#lib/offline/collection.js';
import * as database from '#lib/offline/database.js';
import { queueCollectionUpdate, synchronizeCollection } from '#lib/offline/queue.js';
import { changeOfflineSession } from '#lib/offline/session.js';
import { entry, issue, profile, series, snapshot } from './fixtures/offline';

const rows: CollectionEntry[] = [
	[91, 31, 'Zulu', 1],
	[54, 36, 'Beta', 7],
	[22, 32, 'Älpha', 3],
	[99, 38, 'Gamma', 3],
	[65, 35, 'Älpha', 1],
	[100, 39, 'Beta', 2],
	[44, 34, 'Beta', 1],
	[12, 32, 'Älpha', 3],
	[88, 37, 'Zulu', 2],
	[73, 33, 'alpha', 2]
].map(([id, issueId, name, number]) => ({
	...entry,
	id: Number(id),
	issue_id: Number(issueId),
	series_name: String(name),
	issue_number: Number(number),
	title: 'Shared title'
}));

const authors = new Map([
	[31, ['Émile', 'Anna']],
	[32, ['anna']],
	[33, ['Ánna']],
	[34, ['Béla', 'Zulu']],
	[35, []],
	[36, ['Zoë', '']],
	[38, ['Zoë']],
	[39, ['Anna ', 'Zulu']]
]);

const secondaryOrder = [65, 73, 12, 22, 44, 100, 54, 99, 91, 88];
const ascendingAuthors = [65, 54, 88, 73, 12, 22, 100, 91, 44, 99];
const descendingAuthors = [99, 44, 73, 12, 22, 100, 91, 65, 54, 88];
const ids = (entries: CollectionEntry[]) => entries.map(({ id }) => id);

describe('local collection ordering', () => {
	it.each([
		['author', ascendingAuthors, descendingAuthors],
		['series', secondaryOrder, [91, 88, 99, 44, 100, 54, 65, 73, 12, 22]],
		[
			'issue_number',
			[65, 44, 91, 73, 100, 88, 12, 22, 99, 54],
			[54, 12, 22, 99, 73, 100, 88, 65, 44, 91]
		],
		['title', secondaryOrder, secondaryOrder],
		['condition', secondaryOrder, secondaryOrder],
		['added', secondaryOrder, secondaryOrder]
	] as const)(
		'matches the primary direction and ascending tie-breakers for %s',
		(sort, asc, desc) => {
			expect(ids(filterLocalCollection(rows, { sort, sort_dir: 'asc' }, authors).data)).toEqual(
				asc
			);
			expect(ids(filterLocalCollection(rows, { sort, sort_dir: 'desc' }, authors).data)).toEqual(
				desc
			);
		}
	);

	it.each([undefined, 'unknown'])('uses the default issue-number key for sort=%s', (sort) => {
		expect(ids(filterLocalCollection(rows, { sort }).data)).toEqual([
			65, 44, 91, 73, 100, 88, 12, 22, 99, 54
		]);
	});

	it('keeps collated title ties in ascending secondary order for either direction', () => {
		const entries = rows.map((row, index) => ({
			...row,
			title: ['Équal', 'equal', 'EQUAL '][index % 3]
		}));
		for (const sort_dir of ['asc', 'desc']) {
			expect(ids(filterLocalCollection(entries, { sort: 'title', sort_dir }).data)).toEqual(
				secondaryOrder
			);
		}
	});

	it('uses every author to find the minimum even when filtering by a coauthor', () => {
		expect(
			ids(filterLocalCollection(rows, { sort: 'author', author: 'Zulu' }, authors).data)
		).toEqual([100, 44]);
	});

	it.each([
		['asc', [[65, 54, 88], [73, 12, 22], [100, 91, 44], [99], []]],
		['desc', [[99, 44, 73], [12, 22, 100], [91, 65, 54], [88], []]]
	] as const)('keeps author ties stable across pages in %s order', (sort_dir, pages) => {
		for (const [index, expected] of pages.entries()) {
			const result = filterLocalCollection(
				rows,
				{ sort: 'author', sort_dir, page: index + 1, per_page: 3 },
				authors
			);
			expect(ids(result.data)).toEqual(expected);
			expect(result.total).toBe(10);
		}
	});

	it('does not reorder the cached input entries or author lists', () => {
		const originalRows = structuredClone(rows);
		const originalAuthors = structuredClone(authors);
		filterLocalCollection(rows, { sort: 'author', sort_dir: 'desc' }, authors);
		expect(rows).toEqual(originalRows);
		expect(authors).toEqual(originalAuthors);
	});

	it('sorts missing conditions like SQL FIELD and dates by their actual instant', () => {
		const entries: CollectionEntry[] = [
			{ ...entry, id: 4, series_name: 'Zulu', condition_grade: null, created_at: null },
			{ ...entry, id: 2, series_name: 'Alpha', condition_grade: null, created_at: null },
			{
				...entry,
				id: 3,
				series_name: 'Beta',
				condition_grade: 'Z0',
				created_at: '2026-08-15T01:00:00.001Z'
			},
			{
				...entry,
				id: 1,
				series_name: 'Gamma',
				condition_grade: 'Z4',
				created_at: '2026-08-15T03:00:00+02:00'
			},
			{ ...entry, id: 5, series_name: 'Aardvark', condition_grade: 'Z4' }
		];
		expect(ids(filterLocalCollection(entries, { sort: 'condition' }).data)).toEqual([
			2, 4, 3, 5, 1
		]);
		expect(
			ids(filterLocalCollection(entries, { sort: 'condition', sort_dir: 'desc' }).data)
		).toEqual([5, 1, 3, 2, 4]);
		expect(ids(filterLocalCollection(entries, { sort: 'added' }).data)).toEqual([2, 4, 5, 1, 3]);
		expect(ids(filterLocalCollection(entries, { sort: 'added', sort_dir: 'desc' }).data)).toEqual([
			3, 5, 1, 2, 4
		]);
		expect(
			ids(filterLocalCollection(entries, { condition_min: 'Z0', condition_max: 'Z4' }).data)
		).toEqual([5, 3, 1]);
	});

	it('uses issue IDs as the final ascending tie-breaker for virtual missing entries', () => {
		const missing = [22, 12, 32].map((issue_id) => ({
			...entry,
			id: 0,
			issue_id,
			status: 'missing' as const
		}));
		for (const sort_dir of ['asc', 'desc']) {
			expect(
				filterLocalCollection(missing, { sort: 'author', sort_dir }).data.map((row) => row.issue_id)
			).toEqual([12, 22, 32]);
		}
	});
});

describe('author-ordered responses with local changes', () => {
	beforeEach(async () => {
		localStorage.clear();
		await database.resetOfflineDatabaseForTests();
		changeOfflineSession(1, false);
		await database.saveConfirmedProfile(profile(1));
		await database.replaceOfflineSnapshot({
			...snapshot(1, rows),
			issues: [
				...new Map(
					rows.map((row) => [
						row.issue_id,
						{
							...issue,
							id: row.issue_id,
							issue_number: row.issue_number,
							authors: authors.get(row.issue_id) ?? [],
							cover_artists: ['Aardvark']
						}
					])
				).values()
			],
			series: [series]
		});
	});

	afterEach(() => {
		vi.restoreAllMocks();
		vi.unstubAllGlobals();
	});

	it.each([
		['pending', 'asc', ascendingAuthors],
		['pending', 'desc', descendingAuthors],
		['conflict', 'asc', ascendingAuthors],
		['conflict', 'desc', descendingAuthors]
	] as const)(
		'keeps server page order with a %s edit in %s order',
		async (state, sort_dir, orderedIds) => {
			const queued = await queueCollectionUpdate(1, 73, { notes: 'Unsent local note' });
			if (state === 'conflict') {
				await synchronizeCollection(1, async () => ({
					results: [
						{
							mutation_id: queued.mutation.mutation_id,
							status: 'conflict',
							entry: rows.find((row) => row.id === 73)!,
							error: 'Revision conflict',
							code: 'collection_revision_conflict'
						}
					]
				}));
			}
			const fetch = vi.fn();
			vi.stubGlobal('fetch', fetch);
			for (let page = 1; page <= 5; page++) {
				const expectedIds = orderedIds.slice((page - 1) * 3, page * 3);
				fetch.mockResolvedValueOnce(
					Response.json({
						data: expectedIds.map((id) => rows.find((row) => row.id === id)),
						page,
						per_page: 3,
						total: rows.length
					})
				);
				const result = await fetchCollection({ sort: 'author', sort_dir, page, per_page: 3 });
				expect(ids(result.data)).toEqual(expectedIds);
				expect(result.total).toBe(10);
				const local = result.data.find((row) => row.id === 73);
				if (local) expect(local).toMatchObject({ notes: 'Unsent local note', sync_state: state });
			}
			expect(await database.listMutations(1)).toHaveLength(state === 'pending' ? 1 : 0);
			expect(await database.listConflicts(1)).toHaveLength(state === 'conflict' ? 1 : 0);
		}
	);

	it('uses author metadata and ascending ties for missing issues too', async () => {
		await database.replaceOfflineSnapshot({
			...snapshot(1, []),
			issues: [32, 31, 30].map((id) => ({ ...issue, id, authors: ['Béla', 'Ánna'] }))
		});
		const result = await readLocalCollection(1, {
			status: 'missing',
			sort: 'author',
			sort_dir: 'desc',
			per_page: 2
		});
		expect(result.data.map((row) => row.issue_id)).toEqual([30, 31]);
		expect(result.total).toBe(3);
	});
});
