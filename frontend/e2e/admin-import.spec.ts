import { test, expect } from '@playwright/test';

test.describe.serial('Admin Import Flow', () => {
	test.beforeEach(async ({ page }) => {
		// Login as admin
		await page.goto('/login');
		await page.getByLabel(/e-mail/i).fill('demo@lilly.app');
		await page.getByLabel(/passwort/i).fill('demo1234');
		await page.getByRole('button', { name: /anmelden/i }).click();
		await expect(page).toHaveURL('/', { timeout: 15000 });
	});

	test('import page shows available adapters', async ({ page }) => {
		await page.goto('/admin/import');

		await expect(page.getByTestId('admin-import-title')).toBeVisible();
		await expect(page.getByTestId('start-import-section')).toBeVisible();

		const select = page.getByTestId('adapter-select');
		await expect(select).toBeVisible();

		// The maddrax adapter should be available
		await expect(select.locator('option')).not.toHaveCount(0);
	});

	test('start import creates job and shows progress', async ({ page }) => {
		await page.goto('/admin/import');

		// Wait for adapters to load, then select the first one
		const select = page.getByTestId('adapter-select');
		await expect(select.locator('option')).not.toHaveCount(0);
		await select.selectOption({ index: 0 });

		// Start import
		const startButton = page.getByTestId('start-import-button');
		await expect(startButton).toBeEnabled();
		await startButton.click();

		// Should redirect to import detail page
		await expect(page).toHaveURL(/\/admin\/import\/\d+/, { timeout: 15000 });

		// Import title should be visible
		await expect(page.getByTestId('import-title')).toBeVisible();

		// Progress section should be visible
		await expect(page.getByTestId('progress-section')).toBeVisible();

		// Status should be running or completed (if incremental with 0 new issues)
		const status = page.getByTestId('job-status');
		await expect(status).toBeVisible();
		const statusText = await status.textContent();
		expect(['running', 'completed', 'pending']).toContain(statusText?.trim());
	});

	test('import detail page shows progress count', async ({ page }) => {
		await page.goto('/admin/import');

		// Navigate to the existing import via history (previous test created one)
		const detailsLink = page.getByTestId('view-details-link').first();
		await expect(detailsLink).toBeVisible({ timeout: 10000 });
		await detailsLink.click();
		await expect(page).toHaveURL(/\/admin\/import\/\d+/, { timeout: 15000 });

		// Progress count should be visible
		const progressCount = page.getByTestId('progress-count');
		await expect(progressCount).toBeVisible({ timeout: 30000 });
		const text = await progressCount.textContent();
		expect(text).toMatch(/\d+\s*\/\s*\d+\s*Hefte/);
	});

	test('back link navigates to import overview', async ({ page }) => {
		await page.goto('/admin/import');

		// Navigate to the existing import via history
		const detailsLink = page.getByTestId('view-details-link').first();
		await expect(detailsLink).toBeVisible({ timeout: 10000 });
		await detailsLink.click();
		await expect(page).toHaveURL(/\/admin\/import\/\d+/, { timeout: 15000 });

		// Click back link
		await page.getByTestId('back-link').click();
		await expect(page).toHaveURL(/\/admin\/import$/);
	});

	test('import history shows previous imports', async ({ page }) => {
		await page.goto('/admin/import');

		// After at least one import, history section should exist
		const historySection = page.getByTestId('import-history-section');
		await expect(historySection).toBeVisible();

		// There should be at least one history row or empty message
		const hasRows = await page.getByTestId('history-row').count();
		const hasEmpty = await page
			.getByTestId('empty-history')
			.isVisible()
			.catch(() => false);
		expect(hasRows > 0 || hasEmpty).toBeTruthy();
	});
});
