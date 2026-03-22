import { test, expect, type Page } from '@playwright/test';
import AxeBuilder from '@axe-core/playwright';

/** Shared login helper. */
async function loginAsDemo(page: Page) {
	await page.goto('/login');
	await page.getByTestId('email-input').fill('demo@lilly.app');
	await page.getByTestId('password-input').fill('demo1234');
	await page.getByTestId('submit-button').click();
	await expect(page).toHaveURL('/', { timeout: 15000 });
}

// ---------------------------------------------------------------------------
// Collection page accessibility
// ---------------------------------------------------------------------------

test.describe('Collection Page Accessibility', () => {
	test.beforeEach(async ({ page }) => {
		await loginAsDemo(page);
		await page.goto('/collection');
	});

	test('has no automatically detectable accessibility violations', async ({ page }) => {
		// Wait for content to render
		await expect(page.getByTestId('collection-title')).toBeVisible();
		const results = await new AxeBuilder({ page }).analyze();
		expect(results.violations).toEqual([]);
	});

	test('has a proper heading hierarchy', async ({ page }) => {
		const h1 = page.getByRole('heading', { level: 1 });
		await expect(h1).toBeVisible();
		await expect(h1).toHaveText('Meine Sammlung');
	});

	test('has a descriptive page title', async ({ page }) => {
		await expect(page).toHaveTitle(/Meine Sammlung.*LILLY/);
	});

	test('filter bar labels are accessible', async ({ page }) => {
		// Series dropdown has an associated sr-only label
		const seriesSelect = page.getByLabel(/serie/i);
		await expect(seriesSelect).toBeVisible();

		// Sort dropdown has an associated sr-only label
		const sortSelect = page.getByLabel(/sortierung/i);
		await expect(sortSelect).toBeVisible();

		// Search input has an associated sr-only label
		const searchInput = page.getByLabel(/suche/i);
		await expect(searchInput).toBeVisible();
	});

	test('status filter uses radiogroup role', async ({ page }) => {
		const radiogroup = page.getByRole('radiogroup', { name: /status/i });
		await expect(radiogroup).toBeVisible();

		// Each chip should have role="radio"
		const radios = radiogroup.getByRole('radio');
		const count = await radios.count();
		expect(count).toBeGreaterThanOrEqual(4);
	});

	test('sort direction toggle has descriptive aria-label', async ({ page }) => {
		const toggle = page.getByTestId('sort-dir-toggle');
		const label = await toggle.getAttribute('aria-label');
		expect(label).toMatch(/aufsteigend|absteigend/i);
	});

	test('FAB link is keyboard accessible', async ({ page }) => {
		const fab = page.getByTestId('collection-fab');
		await fab.focus();
		await expect(fab).toBeFocused();
		await expect(fab).toHaveText(/Hinzufügen/);
	});
});

// ---------------------------------------------------------------------------
// Add page accessibility
// ---------------------------------------------------------------------------

test.describe('Add to Collection Page Accessibility', () => {
	test.beforeEach(async ({ page }) => {
		await loginAsDemo(page);
		await page.goto('/collection/add');
	});

	test('has no automatically detectable accessibility violations', async ({ page }) => {
		await expect(page.getByTestId('add-title')).toBeVisible();
		const results = await new AxeBuilder({ page }).analyze();
		expect(results.violations).toEqual([]);
	});

	test('has a proper heading hierarchy', async ({ page }) => {
		const h1 = page.getByRole('heading', { level: 1 });
		await expect(h1).toBeVisible();
		await expect(h1).toHaveText('Serie wählen');
	});

	test('has a descriptive page title', async ({ page }) => {
		await expect(page).toHaveTitle(/Hefte hinzufügen.*LILLY/);
	});

	test('series cards are keyboard navigable', async ({ page }) => {
		await expect(page.getByTestId('loading-indicator')).toBeHidden({ timeout: 10000 });

		const firstCard = page.getByTestId('series-card').first();
		if ((await firstCard.count()) === 0) {
			test.skip();
			return;
		}

		// Series cards are <button> elements, so they should be focusable
		await firstCard.focus();
		await expect(firstCard).toBeFocused();
	});

	test('number cells have descriptive aria-labels', async ({ page }) => {
		await expect(page.getByTestId('loading-indicator')).toBeHidden({ timeout: 10000 });

		const firstCard = page.getByTestId('series-card').first();
		if ((await firstCard.count()) === 0) {
			test.skip();
			return;
		}

		await firstCard.click();
		await expect(page.getByTestId('loading-indicator')).toBeHidden({ timeout: 10000 });

		const firstCell = page.getByTestId('number-cell').first();
		if ((await firstCell.count()) === 0) {
			test.skip();
			return;
		}

		const ariaLabel = await firstCell.getAttribute('aria-label');
		expect(ariaLabel).toBeTruthy();
		// Should contain "Heft #" pattern
		expect(ariaLabel).toMatch(/Heft #\d+/);
	});

	test('toast notification uses role="status"', async ({ page }) => {
		await expect(page.getByTestId('loading-indicator')).toBeHidden({ timeout: 10000 });

		const firstCard = page.getByTestId('series-card').first();
		if ((await firstCard.count()) === 0) {
			test.skip();
			return;
		}

		await firstCard.click();
		await expect(page.getByTestId('loading-indicator')).toBeHidden({ timeout: 10000 });

		const firstCell = page.getByTestId('number-cell').first();
		if ((await firstCell.count()) === 0) {
			test.skip();
			return;
		}

		await firstCell.click();

		const toast = page.getByTestId('toast');
		await expect(toast).toBeVisible({ timeout: 5000 });
		await expect(toast).toHaveAttribute('role', 'status');
	});
});

// ---------------------------------------------------------------------------
// Detail sheet accessibility
// ---------------------------------------------------------------------------

test.describe('Detail Sheet Accessibility', () => {
	test.beforeEach(async ({ page }) => {
		await loginAsDemo(page);
		await page.goto('/collection');
	});

	test('detail sheet uses dialog role with aria-modal', async ({ page }) => {
		await expect(page.getByTestId('cover-grid-skeleton')).toBeHidden({ timeout: 10000 });

		const firstCard = page.getByTestId('cover-card').first();
		if ((await firstCard.count()) === 0) {
			test.skip();
			return;
		}

		await firstCard.click();
		const sheet = page.getByTestId('issue-detail-sheet');
		await expect(sheet).toBeVisible({ timeout: 5000 });

		await expect(sheet).toHaveAttribute('role', 'dialog');
		await expect(sheet).toHaveAttribute('aria-modal', 'true');
	});

	test('detail sheet has descriptive aria-label', async ({ page }) => {
		await expect(page.getByTestId('cover-grid-skeleton')).toBeHidden({ timeout: 10000 });

		const firstCard = page.getByTestId('cover-card').first();
		if ((await firstCard.count()) === 0) {
			test.skip();
			return;
		}

		await firstCard.click();
		const sheet = page.getByTestId('issue-detail-sheet');
		await expect(sheet).toBeVisible({ timeout: 5000 });

		const label = await sheet.getAttribute('aria-label');
		expect(label).toMatch(/Heftdetails:/);
	});

	test('detail sheet status buttons use radiogroup pattern', async ({ page }) => {
		await expect(page.getByTestId('cover-grid-skeleton')).toBeHidden({ timeout: 10000 });

		const firstCard = page.getByTestId('cover-card').first();
		if ((await firstCard.count()) === 0) {
			test.skip();
			return;
		}

		await firstCard.click();
		await expect(page.getByTestId('issue-detail-sheet')).toBeVisible({ timeout: 5000 });

		const radiogroup = page.getByTestId('issue-detail-sheet').getByRole('radiogroup');
		await expect(radiogroup).toBeVisible();

		const radios = radiogroup.getByRole('radio');
		expect(await radios.count()).toBe(3);
	});

	test('Escape key closes detail sheet', async ({ page }) => {
		await expect(page.getByTestId('cover-grid-skeleton')).toBeHidden({ timeout: 10000 });

		const firstCard = page.getByTestId('cover-card').first();
		if ((await firstCard.count()) === 0) {
			test.skip();
			return;
		}

		await firstCard.click();
		await expect(page.getByTestId('issue-detail-sheet')).toBeVisible({ timeout: 5000 });

		await page.keyboard.press('Escape');
		await expect(page.getByTestId('issue-detail-sheet')).toBeHidden({ timeout: 3000 });
	});

	test('backdrop is presentational (not a button)', async ({ page }) => {
		await expect(page.getByTestId('cover-grid-skeleton')).toBeHidden({ timeout: 10000 });

		const firstCard = page.getByTestId('cover-card').first();
		if ((await firstCard.count()) === 0) {
			test.skip();
			return;
		}

		await firstCard.click();
		const backdrop = page.getByTestId('detail-sheet-backdrop');
		await expect(backdrop).toBeVisible({ timeout: 5000 });

		const role = await backdrop.getAttribute('role');
		expect(role).toBe('none');
	});
});
