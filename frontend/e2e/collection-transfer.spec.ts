import { randomUUID } from 'node:crypto';
import { readFile } from 'node:fs/promises';
import AxeBuilder from '@axe-core/playwright';
import { test, expect } from './fixtures';

test('preview, mixed additive import, repeat and complete exports work through the UI', async ({
	page
}, testInfo) => {
	test.setTimeout(90_000);
	const series = (await (await page.request.get('/api/v1/series')).json()) as Array<{
		slug: string;
	}>;
	const selected = series[0].slug;
	const issues = (await (
		await page.request.get(`/api/v1/series/${selected}/issues?per_page=1`)
	).json()) as { data: Array<{ id: number; issue_number: number }> };
	const issue = issues.data[0];
	const key = `e2e-${randomUUID()}`;
	const entry = {
		copy_key: key,
		series_slug: selected,
		issue_number: issue.issue_number,
		copy_number: 200,
		status: 'owned',
		condition_grade: 'Z1',
		edition_label: 'E2E Erstauflage',
		notes: 'Private Notiz; Grüße 🦇\nzweite Zeile'
	};
	const second = {
		...entry,
		copy_key: `${key}-second`,
		copy_number: 201,
		edition_label: 'E2E Variantcover',
		condition_grade: 'Z3'
	};
	const missing = { ...entry, copy_key: `${key}-missing`, issue_number: 999_999 };
	const document = {
		format: 'lilly.collection',
		format_version: 1,
		exported_at: null,
		entries: [entry, second, missing]
	};
	const created: number[] = [];
	try {
		await page.goto('/collection');
		await page.getByTestId('collection-transfer-link').click();
		await page.getByLabel('Dateiformat').selectOption('json');
		await page.getByTestId('collection-import-file').setInputFiles({
			name: 'bestand.json',
			mimeType: 'application/json',
			buffer: Buffer.from(JSON.stringify(document))
		});
		await page.getByTestId('collection-import-preview').click();
		await expect(page.getByTestId('import-counts')).toContainText('2 neu');
		await expect(page.getByTestId('import-counts')).toContainText('1 ungültig');
		const before = (await (
			await page.request.get('/api/v1/me/collection/export?format=json')
		).json()) as { entries: Array<{ copy_key: string }> };
		expect(before.entries.some((item) => item.copy_key === key)).toBe(false);
		await page.getByTestId('collection-import-confirm').click();
		await expect(page.getByRole('heading', { name: 'Import abgeschlossen' })).toBeVisible({
			timeout: 30_000
		});
		await expect(page.getByTestId('import-counts')).toContainText('2 hinzugefügt');
		await page.reload();
		await expect(page.getByRole('heading', { name: 'Import abgeschlossen' })).toBeVisible();
		const collection = (await (
			await page.request.get(`/api/v1/me/collection?issue_id=${issue.id}&per_page=100`)
		).json()) as { data: Array<{ id: number; copy_number: number; notes: string }> };
		created.push(
			...collection.data
				.filter((item) => [200, 201].includes(item.copy_number))
				.map((item) => item.id)
		);
		expect(created).toHaveLength(2);
		await page.getByTestId('collection-import-file').setInputFiles({
			name: 'repeat.json',
			mimeType: 'application/json',
			buffer: Buffer.from(JSON.stringify(document))
		});
		await page.getByTestId('collection-import-preview').click();
		await expect(page.getByRole('heading', { name: 'Vorschau bereit' })).toBeVisible();
		await expect(page.getByTestId('import-counts')).toContainText('2 übersprungen');
		await expect(page.getByTestId('collection-import-confirm')).toBeDisabled();
		const jsonDownload = page.waitForEvent('download');
		await page.getByTestId('collection-export-json').click();
		const jsonPath = testInfo.outputPath('collection.json');
		await (await jsonDownload).saveAs(jsonPath);
		const exported = JSON.parse(await readFile(jsonPath, 'utf8')) as {
			entries: Array<typeof entry>;
		};
		expect(exported.entries.find((item) => item.copy_key === key)).toMatchObject(entry);
		expect(exported.entries.find((item) => item.copy_key === `${key}-second`)).toMatchObject(
			second
		);
		const csvDownload = page.waitForEvent('download');
		await page.getByTestId('collection-export-csv').click();
		const csvPath = testInfo.outputPath('collection.csv');
		await (await csvDownload).saveAs(csvPath);
		const csv = await readFile(csvPath, 'utf8');
		expect(csv).toContain('spreadsheet-safe-v1');
		expect(csv).toContain(key);
		expect(csv).toContain('Grüße');
		const results = page.getByRole('region', { name: 'Import-Ergebnistabelle' });
		await results.focus();
		await expect(results).toBeFocused();
		if (await results.evaluate((element) => element.scrollWidth > element.clientWidth)) {
			await results.press('ArrowRight');
			await expect.poll(() => results.evaluate((element) => element.scrollLeft)).toBeGreaterThan(0);
		}
		const accessibility = await new AxeBuilder({ page }).analyze();
		expect(accessibility.violations).toEqual([]);
	} finally {
		for (const id of created) await page.request.delete(`/api/v1/me/collection/${id}`);
	}
});

test('malformed imports show a useful error without changing the collection', async ({ page }) => {
	await page.goto('/collection/import');
	await expect(page.getByTestId('collection-import-file')).toBeEnabled();
	await page.getByTestId('collection-import-file').setInputFiles({
		name: 'bad.csv',
		mimeType: 'text/csv',
		buffer: Buffer.from('wrong;columns\nsecret;data')
	});
	await page.getByTestId('collection-import-preview').click();
	await expect(page.getByRole('alert')).toContainText('CSV-Spalten');
	await expect(page.getByTestId('collection-import-confirm')).toHaveCount(0);
});
