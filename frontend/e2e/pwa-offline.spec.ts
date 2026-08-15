import { expect, test, unauthenticatedTest } from './fixtures';

unauthenticatedTest(
	'web app manifest is installable and exposes maskable icons',
	async ({ request }) => {
		const response = await request.get('/manifest.webmanifest');
		expect(response.ok()).toBe(true);
		const manifest = (await response.json()) as {
			display: string;
			start_url: string;
			icons: { src: string; sizes: string; purpose?: string }[];
		};
		expect(manifest.display).toBe('standalone');
		expect(manifest.start_url).toBe('/collection');
		expect(manifest.icons.some((icon) => icon.sizes === '192x192')).toBe(true);
		expect(manifest.icons.some((icon) => icon.sizes === '512x512')).toBe(true);
		expect(manifest.icons.some((icon) => icon.purpose === 'maskable')).toBe(true);
	}
);

test('collection reloads offline and a merged create/edit syncs exactly once', async ({
	page,
	context
}) => {
	const editionMarker = `Offline E2E ${Date.now()}`;
	let createdEntryId: number | null = null;

	try {
		await page.goto('/collection');
		await expect(page.getByTestId('collection-title')).toBeVisible();
		await page.evaluate(async () => {
			if (!('serviceWorker' in navigator)) throw new Error('Service workers are unavailable');
			await navigator.serviceWorker.ready;
		});
		await expect
			.poll(() =>
				page.evaluate(
					() =>
						new Promise<number>((resolve, reject) => {
							const request = indexedDB.open('lilly-offline');
							request.onerror = () => reject(request.error);
							request.onsuccess = () => {
								const database = request.result;
								const transaction = database.transaction('issues', 'readonly');
								const count = transaction.objectStore('issues').count();
								count.onerror = () => reject(count.error);
								count.onsuccess = () => {
									database.close();
									resolve(count.result);
								};
							};
						})
				)
			)
			.toBeGreaterThan(0);

		// Visit both offline routes once while controlled so their route chunks are cached.
		await page.goto('/collection/add');
		await expect(page.getByTestId('add-title')).toBeVisible();
		await page.goto('/collection');
		await page.reload();
		await expect(page.getByTestId('collection-title')).toBeVisible();
		await context.setOffline(true);
		await page.reload();
		await expect(page.getByTestId('collection-title')).toBeVisible();
		await expect(page.getByTestId('offline-status')).toContainText('Offline');

		await page.getByTestId('collection-fab').click();
		await expect(page.getByTestId('loading-indicator')).toBeHidden({ timeout: 10000 });
		await page.getByTestId('series-card').first().click();
		await expect(page.getByTestId('series-status-grid')).toBeVisible();
		const missingCell = page
			.locator('[data-testid="series-status-cell"][data-status="missing"]')
			.first();
		await expect(missingCell).toBeVisible();
		const missingCellIndex = await missingCell.evaluate((element) =>
			Array.from(element.parentElement?.children ?? []).indexOf(element)
		);
		const targetCell = page.getByTestId('series-status-cell').nth(missingCellIndex);
		await missingCell.click();
		await page.getByTestId('status-owned').click();
		await page.getByTestId('edition-input').fill(editionMarker);
		await page.getByTestId('notes-textarea').fill('offline angelegt');
		await page.getByTestId('save-button').click();
		await expect(page.getByTestId('toast')).toContainText('hinzugefügt');

		// Edit the still-local entry. This must merge into its create mutation.
		await targetCell.click();
		await page.getByTestId('notes-textarea').fill('offline angelegt und geändert');
		await page.getByTestId('save-button').click();
		await expect(page.getByTestId('toast')).toContainText('aktualisiert');
		await expect(page.getByTestId('offline-status')).toContainText('ausstehend');

		await context.setOffline(false);
		await expect(page.getByTestId('offline-status')).toContainText('Synchronisiert', {
			timeout: 15000
		});

		const collection = await page.request.get('/api/v1/me/collection?per_page=100');
		expect(collection.ok()).toBe(true);
		const body = (await collection.json()) as {
			data: { id: number; edition_label: string | null; notes: string | null }[];
		};
		const matches = body.data.filter((entry) => entry.edition_label === editionMarker);
		expect(matches).toHaveLength(1);
		expect(matches[0].notes).toBe('offline angelegt und geändert');
		createdEntryId = matches[0].id;
	} finally {
		await context.setOffline(false);
		if (createdEntryId !== null) {
			const response = await page.request.delete(`/api/v1/me/collection/${createdEntryId}`);
			expect(response.ok()).toBe(true);
		}
	}
});
