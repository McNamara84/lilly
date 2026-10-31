import { expect, type Page } from '@playwright/test';
import type { CollectionEntry } from '../src/lib/api/collection';

export async function waitForOfflineData(page: Page) {
	await page.evaluate(async () => {
		await navigator.serviceWorker.ready;
	});
	await expect
		.poll(() =>
			page.evaluate(
				() =>
					new Promise<number>((resolve, reject) => {
						const open = indexedDB.open('lilly-offline');
						open.onupgradeneeded = () => {
							open.transaction?.abort();
							resolve(0);
						};
						open.onerror = () =>
							open.error?.name === 'AbortError' ? resolve(0) : reject(open.error);
						open.onsuccess = () => {
							const db = open.result;
							if (!db.objectStoreNames.contains('issues')) {
								db.close();
								resolve(0);
								return;
							}
							const transaction = db.transaction('issues');
							const count = transaction.objectStore('issues').count();
							count.onsuccess = () => resolve(count.result);
							transaction.oncomplete = () => db.close();
						};
					})
			)
		)
		.toBeGreaterThan(100);
}

/** Persist queue fixtures through the real schema; the application performs every sync. */
export async function queueOfflineChanges(
	page: Page,
	count: number,
	operation: 'create' | 'update',
	notes: string
) {
	return page.evaluate(
		async ({ count, operation, notes }) => {
			const db = await new Promise<IDBDatabase>((resolve, reject) => {
				const open = indexedDB.open('lilly-offline');
				open.onsuccess = () => resolve(open.result);
				open.onerror = () => reject(open.error);
			});
			try {
				return await new Promise<string[]>((resolve, reject) => {
					const tx = db.transaction(['collection_entries', 'mutations'], 'readwrite');
					const store = tx.objectStore('collection_entries');
					const entries = store.getAll();
					const ids: string[] = [];
					entries.onsuccess = () => {
						const original = entries.result[0] as CollectionEntry & {
							user_id: number;
							key: string;
						};
						if (!original) {
							tx.abort();
							return;
						}
						for (let index = 0; index < count; index++) {
							const mutationId = crypto.randomUUID();
							ids.push(mutationId);
							const id = operation === 'create' ? -Date.now() - index : original.id;
							const changes = {
								notes: `${notes}${index}`,
								condition_grade: `Z${index % 5}`,
								status: (['owned', 'duplicate', 'wanted'] as const)[index % 3]
							};
							store.put({
								...original,
								...changes,
								id,
								key: `${original.user_id}:${id}`,
								sync_state: 'pending'
							});
							tx.objectStore('mutations').put({
								mutation_id: mutationId,
								user_id: original.user_id,
								operation,
								created_at: new Date(Date.now() + index).toISOString(),
								attempts: 0,
								last_error: null,
								...(operation === 'create'
									? {
											temp_entry_id: id,
											entry: { issue_id: original.issue_id, edition_label: mutationId, ...changes }
										}
									: { entry_id: id, base_revision: original.revision, changes })
							});
						}
					};
					tx.oncomplete = () => {
						window.dispatchEvent(new Event('lilly:offline-change'));
						resolve(ids);
					};
					tx.onabort = () => reject(tx.error ?? new Error('Missing fixture collection'));
				});
			} finally {
				db.close();
			}
		},
		{ count, operation, notes }
	);
}
