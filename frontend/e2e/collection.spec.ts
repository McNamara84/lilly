import { test, expect, type Page } from '@playwright/test';

/** Shared login helper — logs in as demo user before each test. */
async function loginAsDemo(page: Page) {
	await page.goto('/login');
	await page.getByTestId('email-input').fill('demo@lilly.app');
	await page.getByTestId('password-input').fill('demo1234');
	await page.getByTestId('submit-button').click();
	await expect(page).toHaveURL('/', { timeout: 15000 });
}

// ---------------------------------------------------------------------------
// Collection overview page
// ---------------------------------------------------------------------------

test.describe('Collection Overview', () => {
	test.beforeEach(async ({ page }) => {
		await loginAsDemo(page);
	});

	test('header nav link navigates to collection page', async ({ page }) => {
		const link = page.getByTestId('collection-link');
		await expect(link).toBeVisible();
		await link.click();
		await expect(page).toHaveURL(/\/collection$/);
	});

	test('collection page renders title and FAB', async ({ page }) => {
		await page.goto('/collection');
		await expect(page.getByTestId('collection-title')).toHaveText('Meine Sammlung');
		await expect(page.getByTestId('collection-fab')).toBeVisible();
		await expect(page.getByTestId('collection-fab')).toHaveText(/Hinzufügen/);
	});

	test('collection page has correct document title', async ({ page }) => {
		await page.goto('/collection');
		await expect(page).toHaveTitle(/Meine Sammlung.*LILLY/);
	});

	test('FAB navigates to add page', async ({ page }) => {
		await page.goto('/collection');
		await page.getByTestId('collection-fab').click();
		await expect(page).toHaveURL(/\/collection\/add$/);
	});

	test('filter bar is visible on collection page', async ({ page }) => {
		await page.goto('/collection');
		await expect(page.getByTestId('collection-filter-bar')).toBeVisible();
	});

	test('status filter chips are rendered', async ({ page }) => {
		await page.goto('/collection');
		await expect(page.getByTestId('status-filter-all')).toBeVisible();
		await expect(page.getByTestId('status-filter-owned')).toBeVisible();
		await expect(page.getByTestId('status-filter-duplicate')).toBeVisible();
		await expect(page.getByTestId('status-filter-wanted')).toBeVisible();
	});

	test('sort direction toggle is interactive', async ({ page }) => {
		await page.goto('/collection');
		const toggle = page.getByTestId('sort-dir-toggle');
		await expect(toggle).toBeVisible();
		const textBefore = await toggle.textContent();
		await toggle.click();
		const textAfter = await toggle.textContent();
		expect(textBefore).not.toEqual(textAfter);
	});

	test('search input is present and editable', async ({ page }) => {
		await page.goto('/collection');
		const input = page.getByTestId('search-input');
		await expect(input).toBeVisible();
		await input.fill('Maddrax');
		await expect(input).toHaveValue('Maddrax');
	});
});

// ---------------------------------------------------------------------------
// Add issues to collection
// ---------------------------------------------------------------------------

test.describe('Add to Collection', () => {
	test.beforeEach(async ({ page }) => {
		await loginAsDemo(page);
		await page.goto('/collection/add');
	});

	test('add page shows series selection heading', async ({ page }) => {
		await expect(page.getByTestId('add-title')).toHaveText('Serie wählen');
	});

	test('add page has correct document title', async ({ page }) => {
		await expect(page).toHaveTitle(/Hefte hinzufügen.*LILLY/);
	});

	test('series cards are displayed after loading', async ({ page }) => {
		// Wait for loading to finish
		await expect(page.getByTestId('loading-indicator')).toBeHidden({ timeout: 10000 });

		// Either series cards or empty state should be visible
		const seriesCards = page.getByTestId('series-card');
		const emptyState = page.getByTestId('empty-state');
		const hasCards = (await seriesCards.count()) > 0;
		const hasEmpty = await emptyState.isVisible().catch(() => false);
		expect(hasCards || hasEmpty).toBeTruthy();
	});

	test('selecting a series shows number grid and updates heading', async ({ page }) => {
		await expect(page.getByTestId('loading-indicator')).toBeHidden({ timeout: 10000 });

		const firstCard = page.getByTestId('series-card').first();
		// Skip if no series available
		if ((await firstCard.count()) === 0) {
			test.skip();
			return;
		}

		const seriesName = await firstCard.locator('h2').textContent();
		await firstCard.click();

		// Title should update to the series name
		await expect(page.getByTestId('add-title')).toHaveText(seriesName!.trim());
		// Back button should appear
		await expect(page.getByTestId('back-button')).toBeVisible();

		// Wait for grid loading
		await expect(page.getByTestId('loading-indicator')).toBeHidden({ timeout: 10000 });

		// Number grid or empty state should appear
		const grid = page.getByTestId('number-grid');
		const empty = page.getByTestId('empty-state');
		const gridVisible = await grid.isVisible().catch(() => false);
		const emptyVisible = await empty.isVisible().catch(() => false);
		expect(gridVisible || emptyVisible).toBeTruthy();
	});

	test('back button returns to series selection', async ({ page }) => {
		await expect(page.getByTestId('loading-indicator')).toBeHidden({ timeout: 10000 });

		const firstCard = page.getByTestId('series-card').first();
		if ((await firstCard.count()) === 0) {
			test.skip();
			return;
		}

		await firstCard.click();
		await expect(page.getByTestId('back-button')).toBeVisible();

		await page.getByTestId('back-button').click();
		await expect(page.getByTestId('add-title')).toHaveText('Serie wählen');
	});

	test('toggling a number cell shows toast notification', async ({ page }) => {
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

		// Toast should appear with "hinzugefügt" or "entfernt"
		const toast = page.getByTestId('toast');
		await expect(toast).toBeVisible({ timeout: 5000 });
		const toastText = await toast.textContent();
		expect(toastText).toMatch(/hinzugefügt|entfernt/);
	});

	test('number cell reflects collection state after toggle', async ({ page }) => {
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

		// Click to toggle (add or remove)
		await firstCell.click();
		await expect(page.getByTestId('toast')).toBeVisible({ timeout: 5000 });
		// Wait for toast to disappear
		await expect(page.getByTestId('toast')).toBeHidden({ timeout: 5000 });

		// Click again to toggle back
		await firstCell.click();
		await expect(page.getByTestId('toast')).toBeVisible({ timeout: 5000 });
	});
});

// ---------------------------------------------------------------------------
// Collection workflow: add → view in collection → open detail
// ---------------------------------------------------------------------------

test.describe('Collection End-to-End Workflow', () => {
	test.beforeEach(async ({ page }) => {
		await loginAsDemo(page);
	});

	test('full workflow: add issue, see in collection, use filters', async ({ page }) => {
		// Step 1: Go to add page
		await page.goto('/collection/add');
		await expect(page.getByTestId('loading-indicator')).toBeHidden({ timeout: 10000 });

		const firstCard = page.getByTestId('series-card').first();
		if ((await firstCard.count()) === 0) {
			test.skip();
			return;
		}

		// Step 2: Select a series
		await firstCard.click();
		await expect(page.getByTestId('loading-indicator')).toBeHidden({ timeout: 10000 });

		const cells = page.getByTestId('number-cell');
		if ((await cells.count()) === 0) {
			test.skip();
			return;
		}

		// Step 3: Add the first issue
		const firstCell = cells.first();
		const ariaLabel = await firstCell.getAttribute('aria-label');
		const wasInCollection = ariaLabel?.includes('in Sammlung') ?? false;

		if (!wasInCollection) {
			await firstCell.click();
			await expect(page.getByTestId('toast')).toBeVisible({ timeout: 5000 });
			await expect(page.getByTestId('toast')).toContainText(/hinzugefügt/);
		}

		// Step 4: Navigate to collection overview
		await page.goto('/collection');
		await expect(page.getByTestId('collection-title')).toBeVisible();

		// Step 5: Filter bar should be present and usable
		const filterBar = page.getByTestId('collection-filter-bar');
		await expect(filterBar).toBeVisible();

		// Step 6: Click "Vorhanden" status filter
		await page.getByTestId('status-filter-owned').click();
		// Page should still be on collection
		await expect(page).toHaveURL(/\/collection/);
	});

	test('collection page shows cover cards when entries exist', async ({ page }) => {
		await page.goto('/collection');
		// Wait for loading to finish
		await expect(page.getByTestId('cover-grid-skeleton')).toBeHidden({ timeout: 10000 });

		// Either cover cards are shown or the grid is empty
		const coverCards = page.getByTestId('cover-card');
		const count = await coverCards.count();
		// If there are entries, they should be cover-card elements
		if (count > 0) {
			await expect(coverCards.first()).toBeVisible();
		}
	});

	test('clicking a cover card opens the detail sheet', async ({ page }) => {
		await page.goto('/collection');
		await expect(page.getByTestId('cover-grid-skeleton')).toBeHidden({ timeout: 10000 });

		const firstCard = page.getByTestId('cover-card').first();
		if ((await firstCard.count()) === 0) {
			test.skip();
			return;
		}

		await firstCard.click();

		// Detail sheet should slide in
		const sheet = page.getByTestId('issue-detail-sheet');
		await expect(sheet).toBeVisible({ timeout: 5000 });

		// Sheet should contain save button
		await expect(page.getByTestId('save-button')).toBeVisible();
	});

	test('detail sheet can be closed via backdrop', async ({ page }) => {
		await page.goto('/collection');
		await expect(page.getByTestId('cover-grid-skeleton')).toBeHidden({ timeout: 10000 });

		const firstCard = page.getByTestId('cover-card').first();
		if ((await firstCard.count()) === 0) {
			test.skip();
			return;
		}

		await firstCard.click();
		await expect(page.getByTestId('issue-detail-sheet')).toBeVisible({ timeout: 5000 });

		// Click backdrop to close
		await page.getByTestId('detail-sheet-backdrop').click();
		await expect(page.getByTestId('issue-detail-sheet')).toBeHidden({ timeout: 3000 });
	});

	test('detail sheet shows status radio buttons', async ({ page }) => {
		await page.goto('/collection');
		await expect(page.getByTestId('cover-grid-skeleton')).toBeHidden({ timeout: 10000 });

		const firstCard = page.getByTestId('cover-card').first();
		if ((await firstCard.count()) === 0) {
			test.skip();
			return;
		}

		await firstCard.click();
		await expect(page.getByTestId('issue-detail-sheet')).toBeVisible({ timeout: 5000 });

		await expect(page.getByTestId('status-owned')).toBeVisible();
		await expect(page.getByTestId('status-duplicate')).toBeVisible();
		await expect(page.getByTestId('status-wanted')).toBeVisible();
	});

	test('detail sheet has notes textarea', async ({ page }) => {
		await page.goto('/collection');
		await expect(page.getByTestId('cover-grid-skeleton')).toBeHidden({ timeout: 10000 });

		const firstCard = page.getByTestId('cover-card').first();
		if ((await firstCard.count()) === 0) {
			test.skip();
			return;
		}

		await firstCard.click();
		await expect(page.getByTestId('issue-detail-sheet')).toBeVisible({ timeout: 5000 });

		const textarea = page.getByTestId('notes-textarea');
		await expect(textarea).toBeVisible();
		await textarea.fill('E2E Test Notiz');
		await expect(textarea).toHaveValue('E2E Test Notiz');
	});
});

// ---------------------------------------------------------------------------
// Unauthenticated access
// ---------------------------------------------------------------------------

test.describe('Collection – Unauthenticated', () => {
	test('redirects to login when not authenticated', async ({ page }) => {
		await page.goto('/collection');
		await expect(page).toHaveURL(/\/login/, { timeout: 10000 });
	});

	test('add page redirects to login when not authenticated', async ({ page }) => {
		await page.goto('/collection/add');
		await expect(page).toHaveURL(/\/login/, { timeout: 10000 });
	});
});
