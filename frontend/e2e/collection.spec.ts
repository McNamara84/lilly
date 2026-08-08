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

	test('combined metadata filters survive reload and can be reset', async ({ page }) => {
		await page.goto('/collection');
		const advancedToggle = page.getByTestId('advanced-filter-toggle');
		if (await advancedToggle.isVisible()) await advancedToggle.click();

		await page.getByLabel('Serie').selectOption('maddrax');
		await page.getByTestId('issue-number-filter').fill('1');
		await page.getByTestId('condition-filter').selectOption('Z1');
		await page.getByTestId('title-filter').fill('Gott aus dem Eis');
		await page.getByTestId('author-filter').fill('Zybell');
		await page.getByLabel('Sortierung').selectOption('author');
		await page.getByTestId('sort-dir-toggle').click();

		await expect
			.poll(() => Object.fromEntries(new URL(page.url()).searchParams))
			.toEqual({
				series_slug: 'maddrax',
				issue_number: '1',
				condition: 'Z1',
				title: 'Gott aus dem Eis',
				author: 'Zybell',
				sort: 'author',
				sort_dir: 'desc'
			});

		await page.reload();
		await expect(page.getByLabel('Serie')).toHaveValue('maddrax');
		await expect(page.getByTestId('issue-number-filter')).toHaveValue('1');
		await expect(page.getByTestId('condition-filter')).toHaveValue('Z1');
		await expect(page.getByTestId('title-filter')).toHaveValue('Gott aus dem Eis');
		await expect(page.getByTestId('author-filter')).toHaveValue('Zybell');
		await expect(page.getByLabel('Sortierung')).toHaveValue('author');
		await expect(page.getByTestId('sort-dir-toggle')).toHaveText('↓');

		await page.getByTestId('reset-filters').click();
		await expect(page).toHaveURL(/\/collection$/);
		await expect(page.getByTestId('issue-number-filter')).toHaveValue('');
		await expect(page.getByTestId('title-filter')).toHaveValue('');
		await expect(page.getByTestId('author-filter')).toHaveValue('');
	});

	test('browser back restores the previous filter state', async ({ page }) => {
		await page.goto('/collection');
		await page.getByLabel('Serie').selectOption('maddrax');
		await expect(page).toHaveURL(/\/collection\?series_slug=maddrax$/);

		await page.getByTestId('status-filter-owned').click();
		await expect(page).toHaveURL(/series_slug=maddrax&status=owned/);

		await page.goBack();
		await expect(page).toHaveURL(/\/collection\?series_slug=maddrax$/);
		await expect(page.getByLabel('Serie')).toHaveValue('maddrax');
		await expect(page.getByTestId('status-filter-all')).toHaveAttribute('aria-checked', 'true');
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
		await expect(page.getByTestId('loading-indicator')).toBeHidden({ timeout: 10000 });
		await expect(page.getByTestId('series-card').first()).toBeVisible();
	});

	test('selecting a series shows number grid and updates heading', async ({ page }) => {
		await expect(page.getByTestId('loading-indicator')).toBeHidden({ timeout: 10000 });

		const firstCard = page.getByTestId('series-card').first();
		await expect(firstCard).toBeVisible();

		const seriesName = await firstCard.locator('h2').textContent();
		await firstCard.click();

		// Title should update to the series name
		await expect(page.getByTestId('add-title')).toHaveText(seriesName!.trim());
		// Back button should appear
		await expect(page.getByTestId('back-button')).toBeVisible();

		// Wait for grid loading
		await expect(page.getByTestId('loading-indicator')).toBeHidden({ timeout: 10000 });

		await expect(page.getByTestId('number-grid')).toBeVisible();
		await expect(page.getByTestId('number-cell').first()).toBeVisible();
	});

	test('back button returns to series selection', async ({ page }) => {
		await expect(page.getByTestId('loading-indicator')).toBeHidden({ timeout: 10000 });

		const firstCard = page.getByTestId('series-card').first();
		await expect(firstCard).toBeVisible();

		await firstCard.click();
		await expect(page.getByTestId('back-button')).toBeVisible();

		await page.getByTestId('back-button').click();
		await expect(page.getByTestId('add-title')).toHaveText('Serie wählen');
	});

	test('toggling a number cell shows toast notification', async ({ page }) => {
		await expect(page.getByTestId('loading-indicator')).toBeHidden({ timeout: 10000 });

		const firstCard = page.getByTestId('series-card').first();
		await expect(firstCard).toBeVisible();

		await firstCard.click();
		await expect(page.getByTestId('loading-indicator')).toBeHidden({ timeout: 10000 });

		const firstCell = page.getByTestId('number-cell').first();
		await expect(firstCell).toBeVisible();

		await firstCell.click();

		// Toast should appear with "hinzugefügt" or "entfernt"
		const toast = page.getByTestId('toast');
		await expect(toast).toBeVisible({ timeout: 5000 });
		const toastText = await toast.textContent();
		expect(toastText).toMatch(/hinzugefügt|entfernt/);

		// Restore the initial state so this test is independent of later scenarios.
		await expect(toast).toBeHidden({ timeout: 5000 });
		await firstCell.click();
		await expect(toast).toBeVisible({ timeout: 5000 });
	});

	test('number cell reflects collection state after toggle', async ({ page }) => {
		await expect(page.getByTestId('loading-indicator')).toBeHidden({ timeout: 10000 });

		const firstCard = page.getByTestId('series-card').first();
		await expect(firstCard).toBeVisible();

		await firstCard.click();
		await expect(page.getByTestId('loading-indicator')).toBeHidden({ timeout: 10000 });

		const firstCell = page.getByTestId('number-cell').first();
		await expect(firstCell).toBeVisible();

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
		await expect(firstCard).toBeVisible();

		// Step 2: Select a series
		await firstCard.click();
		await expect(page.getByTestId('loading-indicator')).toBeHidden({ timeout: 10000 });

		const cells = page.getByTestId('number-cell');
		await expect(cells.first()).toBeVisible();

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

		const coverCards = page.getByTestId('cover-card');
		await expect(coverCards.first()).toBeVisible();
	});

	test('clicking a cover card opens the detail sheet', async ({ page }) => {
		await page.goto('/collection');
		await expect(page.getByTestId('cover-grid-skeleton')).toBeHidden({ timeout: 10000 });

		const firstCard = page.getByTestId('cover-card').first();
		await expect(firstCard).toBeVisible();

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
		await expect(firstCard).toBeVisible();

		await firstCard.click();
		await expect(page.getByTestId('issue-detail-sheet')).toBeVisible({ timeout: 5000 });

		// Click backdrop to close
		await page.getByTestId('detail-sheet-backdrop').click({ position: { x: 10, y: 10 } });
		await expect(page.getByTestId('issue-detail-sheet')).toBeHidden({ timeout: 3000 });
	});

	test('detail sheet shows status radio buttons', async ({ page }) => {
		await page.goto('/collection');
		await expect(page.getByTestId('cover-grid-skeleton')).toBeHidden({ timeout: 10000 });

		const firstCard = page.getByTestId('cover-card').first();
		await expect(firstCard).toBeVisible();

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
		await expect(firstCard).toBeVisible();

		await firstCard.click();
		await expect(page.getByTestId('issue-detail-sheet')).toBeVisible({ timeout: 5000 });

		const textarea = page.getByTestId('notes-textarea');
		await expect(textarea).toBeVisible();
		await textarea.fill('E2E Test Notiz');
		await expect(textarea).toHaveValue('E2E Test Notiz');
	});

	test('dashboard progress follows owned, wanted and duplicate status changes', async ({
		page
	}) => {
		async function changeFirstEntryStatus(status: 'owned' | 'duplicate' | 'wanted') {
			await page.goto('/collection');
			await expect(page.getByTestId('cover-grid-skeleton')).toBeHidden({ timeout: 10000 });
			await page.getByTestId('cover-card').first().click();
			await expect(page.getByTestId('issue-detail-sheet')).toBeVisible({ timeout: 5000 });
			await page.getByTestId(`status-${status}`).click();
			await page.getByTestId('save-button').click();
			await expect(page.getByTestId('issue-detail-sheet')).toBeHidden({ timeout: 5000 });
		}

		try {
			await changeFirstEntryStatus('wanted');
			await page.goto('/');
			const wantedProgress = page.getByTestId('series-progress-bar').first();
			await expect(wantedProgress).toContainText(/0 von \d+ — 0\.0%/);
			await expect(wantedProgress.getByRole('progressbar')).toHaveAttribute('aria-valuenow', '0');

			await changeFirstEntryStatus('duplicate');
			await page.goto('/');
			const duplicateProgress = page.getByTestId('series-progress-bar').first();
			await expect(duplicateProgress).toContainText('1 Doppelte');
			const duplicateBar = duplicateProgress.getByRole('progressbar');
			const progressLabel = await duplicateBar.getAttribute('aria-label');
			const totalMatch = progressLabel?.match(/1 von (\d+) Heften/);
			expect(totalMatch).not.toBeNull();
			const expectedPercent = 100 / Number(totalMatch![1]);
			await expect(duplicateProgress).toContainText(`1 von ${totalMatch![1]}`);
			expect(Number(await duplicateBar.getAttribute('aria-valuenow'))).toBeCloseTo(
				expectedPercent,
				8
			);
		} finally {
			await changeFirstEntryStatus('owned');
		}
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
