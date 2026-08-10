import type { Page } from '@playwright/test';
import { expect, test } from './fixtures';

async function startFixtureImport(page: Page) {
	await page.goto('/admin/import');
	const select = page.getByTestId('adapter-select');
	await expect(select.locator('option[value="e2e-fixture"]')).toHaveCount(1);
	await select.selectOption('e2e-fixture');

	const startButton = page.getByTestId('start-import-button');
	await expect(startButton).toBeEnabled();
	await startButton.click();
	await expect(page).toHaveURL(/\/admin\/import\/\d+/, { timeout: 15000 });
}

test.describe('Admin Import Flow', () => {
	test.describe.configure({ mode: 'serial' });

	test('import page shows available adapters', async ({ page }) => {
		await page.goto('/admin/import');

		await expect(page.getByTestId('admin-import-title')).toBeVisible();
		await expect(page.getByTestId('start-import-section')).toBeVisible();

		const select = page.getByTestId('adapter-select');
		await expect(select).toBeVisible();
		await expect(select.locator('option[value="e2e-fixture"]')).toHaveCount(1);
	});

	test('start import creates job and shows progress', async ({ page }) => {
		await startFixtureImport(page);

		await expect(page.getByTestId('import-title')).toBeVisible();
		await expect(page.getByTestId('progress-section')).toBeVisible();

		const status = page.getByTestId('job-status');
		await expect(status).toBeVisible();
		const statusText = await status.textContent();
		expect(['pending', 'running', 'completed', 'completed_with_errors']).toContain(
			statusText?.trim()
		);
	});

	test('import detail page shows progress count', async ({ page }) => {
		await startFixtureImport(page);

		const progressCount = page.getByTestId('progress-count');
		await expect(progressCount).toBeVisible({ timeout: 15000 });
		await expect(progressCount).toHaveText(/\d+\s*\/\s*\d+\s*bearbeitet/);
	});

	test('back link navigates to import overview', async ({ page }) => {
		await startFixtureImport(page);

		await page.getByTestId('back-link').click();
		await expect(page).toHaveURL(/\/admin\/import$/);
	});

	test('import history shows the newly created import', async ({ page }) => {
		await startFixtureImport(page);
		await page.goto('/admin/import');

		await expect(page.getByTestId('import-history-section')).toBeVisible();
		await expect(page.getByTestId('history-row').first()).toBeVisible();
	});

	test('imports, reviews, activates and publishes the fixture series', async ({ page }) => {
		const deactivateResponse = await page.request.post(
			'/api/v1/admin/series/e2e-fixture-series/deactivate'
		);
		expect([200, 404]).toContain(deactivateResponse.status());

		try {
			await startFixtureImport(page);

			await expect(page.getByTestId('job-status')).toHaveText(
				/^(completed|completed_with_errors)$/,
				{ timeout: 30000 }
			);
			await expect(page.getByTestId('review-section')).toBeVisible();
			await expect(page.getByTestId('issue-row')).toHaveCount(1);
			await expect(page.getByTestId('issue-row')).toContainText('Deterministic E2E Issue');
			await expect(page.getByTestId('issue-row')).toContainText('LILLY Test Suite');

			const acknowledgement = page.getByTestId('warning-acknowledgement').getByRole('checkbox');
			const activateButton = page.getByTestId('activate-series-button');
			await expect(acknowledgement).toBeVisible();
			await expect(activateButton).toBeDisabled();
			await acknowledgement.check();
			await expect(activateButton).toBeEnabled();
			await activateButton.click();

			await expect(page.getByTestId('series-active-message')).toHaveText(
				'Serie ist veröffentlicht.'
			);

			const publicSeriesResponse = await page.request.get('/api/v1/series');
			expect(publicSeriesResponse.ok()).toBe(true);
			const publicSeries = (await publicSeriesResponse.json()) as Array<{
				slug: string;
				active: boolean;
			}>;
			expect(publicSeries).toContainEqual(
				expect.objectContaining({ slug: 'e2e-fixture-series', active: true })
			);

			const publicIssuesResponse = await page.request.get(
				'/api/v1/series/e2e-fixture-series/issues'
			);
			expect(publicIssuesResponse.ok()).toBe(true);
			const publicIssues = (await publicIssuesResponse.json()) as {
				data: Array<{ issue_number: number; title: string; authors: string[] }>;
				total: number;
			};
			expect(publicIssues.total).toBe(1);
			expect(publicIssues.data).toEqual([
				expect.objectContaining({
					issue_number: 1,
					title: 'Deterministic E2E Issue',
					authors: ['LILLY Test Suite']
				})
			]);
		} finally {
			const cleanupResponse = await page.request.post(
				'/api/v1/admin/series/e2e-fixture-series/deactivate'
			);
			expect([200, 404]).toContain(cleanupResponse.status());
		}
	});
});
