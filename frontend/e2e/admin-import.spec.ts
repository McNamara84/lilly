import { test, expect } from '@playwright/test';

test.describe.serial('Admin Import Flow', () => {
	test.beforeEach(async ({ page }) => {
		// Login as admin
		await page.goto('/login');
		await page.getByTestId('email-input').fill('demo@lilly.app');
		await page.getByTestId('password-input').fill('demo1234');
		await page.getByTestId('submit-button').click();
		await expect(page).toHaveURL('/', { timeout: 15000 });
	});

	test('import page shows available adapters', async ({ page }) => {
		await page.goto('/admin/import');

		await expect(page.getByTestId('admin-import-title')).toBeVisible();
		await expect(page.getByTestId('start-import-section')).toBeVisible();

		const select = page.getByTestId('adapter-select');
		await expect(select).toBeVisible();

		// The test uses Maddrax explicitly so adapter registry ordering cannot affect it.
		await expect(select.locator('option[value="maddrax"]')).toHaveCount(1);
	});

	test('start import creates job and shows progress', async ({ page }) => {
		await page.goto('/admin/import');

		// Wait for adapters to load, then select a deterministic adapter.
		const select = page.getByTestId('adapter-select');
		await expect(select.locator('option[value="maddrax"]')).toHaveCount(1);
		await select.selectOption('maddrax');

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

		// The external source may finish or fail before the detail request completes.
		// This flow verifies that the created job is rendered in every valid state.
		const status = page.getByTestId('job-status');
		await expect(status).toBeVisible();
		const statusText = await status.textContent();
		expect(['pending', 'running', 'completed', 'completed_with_errors', 'failed']).toContain(
			statusText?.trim()
		);
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
		expect(text).toMatch(/\d+\s*\/\s*\d+\s*bearbeitet/);
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
