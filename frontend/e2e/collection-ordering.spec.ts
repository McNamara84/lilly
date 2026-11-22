import { expect, test } from './fixtures';
import type { Page } from '@playwright/test';
import type { CollectionEntry, PaginatedCollection } from '../src/lib/api/collection';
import type {
	CollectionMutation,
	OfflineSnapshot,
	StoredCollectionEntry,
	StoredConflict
} from '../src/lib/offline/types';

test.use({ serviceWorkers: 'block' });
// Allow the large fixture's IndexedDB writes to finish before UI comparisons begin.
test.setTimeout(process.env.PLAYWRIGHT_COLLECTION_TOTAL ? 600_000 : 120_000);

async function cacheSnapshot(page: Page, snapshot: OfflineSnapshot) {
	// Prepare complete reference data in bounded batches before testing local page order.
	for (const [store, records] of [
		['series', snapshot.series],
		['issues', snapshot.issues],
		['collection_entries', snapshot.collection_entries]
	] as const) {
		for (let offset = 0; offset < records.length; offset += 128) {
			await page.evaluate(
				({ store, rows, userId }) =>
					new Promise<void>((resolve, reject) => {
						const open = indexedDB.open('lilly-offline');
						open.onerror = () => reject(open.error);
						open.onsuccess = () => {
							const db = open.result;
							const tx = db.transaction(store, 'readwrite');
							for (const row of rows)
								tx.objectStore(store).put({ ...row, key: `${userId}:${row.id}`, user_id: userId });
							tx.oncomplete = () => {
								db.close();
								resolve();
							};
							tx.onabort = () => {
								db.close();
								reject(tx.error);
							};
						};
					}),
				{ store, rows: records.slice(offset, offset + 128), userId: snapshot.user_id }
			);
		}
	}
}

const orderingTest = test.extend<{ orderingEntries: void }>({
	orderingEntries: [
		async ({ page }, use) => {
			const response = await page.request.get('/api/v1/me/collection?per_page=1');
			expect(response.ok()).toBe(true);
			const original: PaginatedCollection = await response.json();
			expect(original.data.length).toBeGreaterThan(0);
			const created: number[] = [];
			try {
				for (let count = original.total; count < 6; count++) {
					const response = await page.request.post('/api/v1/me/collection', {
						data: { issue_id: original.data[0].issue_id, status: 'owned', condition_grade: 'Z1' }
					});
					expect(response.ok()).toBe(true);
					created.push((await response.json()).id);
				}
				// Keep this ordering fixture stable while normal collection GETs succeed.
				await page.route('**/api/v1/me/collection/offline-snapshot', (route) => route.abort());
				await page.route('**/api/v1/me/collection/sync', (route) =>
					route.fulfill({
						status: 500,
						json: { error: 'Synchronization paused by the ordering test' }
					})
				);
				const response = await page.request.get('/api/v1/me/collection/offline-snapshot');
				expect(response.ok()).toBe(true);
				const snapshot: OfflineSnapshot = await response.json();
				await page.goto('/collection');
				await expect(page.getByTestId('cover-card').first()).toBeVisible();
				await cacheSnapshot(page, snapshot);
				await use();
			} finally {
				for (const id of created)
					expect((await page.request.delete(`/api/v1/me/collection/${id}`)).ok()).toBe(true);
			}
		},
		{ auto: true }
	]
});

orderingTest(
	'preserves API author ordering and pagination with pending and conflicted local edits',
	async ({ page }) => {
		for (const state of ['pending', 'conflict'] as const) {
			const response = await page.request.get('/api/v1/me/collection?sort=author&per_page=3');
			expect(response.ok()).toBe(true);
			const firstPage: PaginatedCollection = await response.json();
			expect(firstPage.total).toBeGreaterThan(3);
			const target = firstPage.data[0];
			const notes = `Local ordering ${state}`;
			await page.evaluate(
				async ({ target, state, notes }) => {
					const db = await new Promise<IDBDatabase>((resolve, reject) => {
						const open = indexedDB.open('lilly-offline');
						open.onsuccess = () => resolve(open.result);
						open.onerror = () => reject(open.error);
					});
					try {
						await new Promise<void>((resolve, reject) => {
							const tx = db.transaction(
								['collection_entries', 'mutations', 'conflicts'],
								'readwrite'
							);
							tx.objectStore('mutations').clear();
							tx.objectStore('conflicts').clear();
							const store = tx.objectStore('collection_entries');
							const request = store.getAll();
							request.onsuccess = () => {
								const original = (request.result as StoredCollectionEntry[]).find(
									({ id }) => id === target.id
								);
								if (!original) {
									tx.abort();
									return;
								}
								const mutation: CollectionMutation = {
									mutation_id: crypto.randomUUID(),
									user_id: original.user_id,
									created_at: new Date().toISOString(),
									attempts: 0,
									last_error: null,
									operation: 'update',
									entry_id: original.id,
									base_revision: original.revision ?? 0,
									changes: { notes }
								};
								store.put({ ...original, notes, sync_state: state });
								if (state === 'pending') tx.objectStore('mutations').put(mutation);
								else
									tx.objectStore('conflicts').put({
										mutation_id: mutation.mutation_id,
										user_id: original.user_id,
										created_at: mutation.created_at,
										mutation,
										server_entry: target,
										error: 'Revision conflict',
										code: 'collection_revision_conflict',
										status: 'conflict'
									} satisfies StoredConflict);
							};
							tx.oncomplete = () => resolve();
							tx.onabort = () => reject(tx.error ?? new Error('Missing cached collection entry'));
						});
					} finally {
						db.close();
					}
				},
				{ target, state, notes }
			);

			for (const direction of ['asc', 'desc']) {
				for (const pageNumber of [1, 2]) {
					const params = new URLSearchParams({
						sort: 'author',
						sort_dir: direction,
						page: String(pageNumber),
						per_page: '3'
					});
					const response = await page.request.get(`/api/v1/me/collection?${params}`);
					expect(response.ok()).toBe(true);
					const expected: PaginatedCollection = await response.json();
					await page.goto(`/collection?${params}`);
					await expect
						.poll(
							() =>
								page
									.getByTestId('cover-card')
									.evaluateAll((cards) =>
										cards.map((card) => Number(card.getAttribute('data-entry-id')))
									),
							{ timeout: 15_000 }
						)
						.toEqual(expected.data.map(({ id }) => id));
				}
			}
			const retained = await page.evaluate(
				(targetId) =>
					new Promise<CollectionEntry | undefined>((resolve, reject) => {
						const open = indexedDB.open('lilly-offline');
						open.onerror = () => reject(open.error);
						open.onsuccess = () => {
							const db = open.result;
							const tx = db.transaction('collection_entries');
							const request = tx.objectStore('collection_entries').getAll();
							request.onsuccess = () =>
								resolve((request.result as CollectionEntry[]).find(({ id }) => id === targetId));
							request.onerror = () => reject(request.error);
							tx.oncomplete = () => db.close();
						};
					}),
				target.id
			);
			expect(retained).toMatchObject({
				notes,
				sync_state: state
			});
		}
	}
);
