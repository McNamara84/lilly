import { expect, test, unauthenticatedTest } from './fixtures';

// ---------------------------------------------------------------------------
// Collection overview page
// ---------------------------------------------------------------------------

test.describe('Collection Overview', () => {
	test.beforeEach(async ({ page }) => {
		await page.goto('/');
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
		await page.goto('/collection/add');
	});

	test('add page shows series selection heading', async ({ page }) => {
		await expect(page.getByTestId('add-title')).toHaveText('Serie wählen');
	});

	test('add page has correct document title', async ({ page }) => {
		await expect(page).toHaveTitle(/Serienraster.*LILLY/);
	});

	test('series cards are displayed after loading', async ({ page }) => {
		await expect(page.getByTestId('loading-indicator')).toBeHidden({ timeout: 10000 });
		await expect(page.getByTestId('series-card').first()).toBeVisible();
	});

	test('selecting a series shows the four-state grid and updates heading', async ({ page }) => {
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

		await expect(page.getByTestId('series-status-grid')).toBeVisible();
		await expect(page.getByTestId('series-status-cell').first()).toBeVisible();
		for (const status of ['owned', 'duplicate', 'wanted', 'missing']) {
			await expect(page.getByTestId(`legend-${status}`)).toBeVisible();
		}
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

	test('grid remains free of horizontal page scrolling at phone, tablet and desktop widths', async ({
		page
	}) => {
		await expect(page.getByTestId('loading-indicator')).toBeHidden({ timeout: 10000 });
		await page.getByTestId('series-card').first().click();
		await expect(page.getByTestId('series-status-grid')).toBeVisible();

		for (const width of [375, 768, 1280]) {
			await page.setViewportSize({ width, height: 800 });
			const horizontalOverflow = await page.evaluate(
				() => document.documentElement.scrollWidth - document.documentElement.clientWidth
			);
			expect(horizontalOverflow).toBeLessThanOrEqual(0);
		}
	});

	test('selecting a grid cell opens details without changing its status', async ({ page }) => {
		await expect(page.getByTestId('loading-indicator')).toBeHidden({ timeout: 10000 });

		const firstCard = page.getByTestId('series-card').first();
		await expect(firstCard).toBeVisible();

		await firstCard.click();
		await expect(page.getByTestId('loading-indicator')).toBeHidden({ timeout: 10000 });

		const firstCell = page.getByTestId('series-status-cell').first();
		await expect(firstCell).toBeVisible();
		const initialStatus = await firstCell.getAttribute('data-status');

		await firstCell.click();

		await expect(page.getByTestId('issue-detail-sheet')).toBeVisible();
		await expect(firstCell).toHaveAttribute('data-status', initialStatus!);
		await expect(page.getByTestId('toast')).toHaveCount(0);
	});

	test('saving details updates the grid state without a reload and can be restored', async ({
		page
	}) => {
		await expect(page.getByTestId('loading-indicator')).toBeHidden({ timeout: 10000 });

		const firstCard = page.getByTestId('series-card').first();
		await expect(firstCard).toBeVisible();

		await firstCard.click();
		await expect(page.getByTestId('loading-indicator')).toBeHidden({ timeout: 10000 });

		const firstCell = page.getByTestId('series-status-cell').first();
		await expect(firstCell).toBeVisible();
		const originalStatus = await firstCell.getAttribute('data-status');

		try {
			await firstCell.click();
			await expect(page.getByTestId('issue-detail-sheet')).toBeVisible();
			await page.getByTestId('status-wanted').click();
			await page.getByTestId('save-button').click();
			await expect(page.getByTestId('issue-detail-sheet')).toBeHidden({ timeout: 5000 });
			await expect(firstCell).toHaveAttribute('data-status', 'wanted');
			await expect(page.getByTestId('toast')).toHaveAttribute('role', 'status');
		} finally {
			await firstCell.click();
			await expect(page.getByTestId('issue-detail-sheet')).toBeVisible();
			if (originalStatus === 'missing') {
				await page.getByTestId('delete-button').click();
				await expect(firstCell).toHaveAttribute('data-status', 'missing');
			} else {
				await page.getByTestId(`status-${originalStatus}`).click();
				await page.getByTestId('save-button').click();
				await expect(firstCell).toHaveAttribute('data-status', originalStatus!);
			}
		}
	});
});

// ---------------------------------------------------------------------------
// Collection workflow: add → view in collection → open detail
// ---------------------------------------------------------------------------

test.describe('Collection End-to-End Workflow', () => {
	test('full workflow: add issue, see in collection, use filters', async ({ page }) => {
		// Step 1: Go to add page
		await page.goto('/collection/add');
		await expect(page.getByTestId('loading-indicator')).toBeHidden({ timeout: 10000 });

		const firstCard = page.getByTestId('series-card').first();
		await expect(firstCard).toBeVisible();

		// Step 2: Select a series
		await firstCard.click();
		await expect(page.getByTestId('loading-indicator')).toBeHidden({ timeout: 10000 });

		const cells = page.getByTestId('series-status-cell');
		await expect(cells.first()).toBeVisible();

		// Step 3: Add the first issue
		const firstCell = cells.first();
		const wasInCollection = (await firstCell.getAttribute('data-status')) !== 'missing';

		if (!wasInCollection) {
			await firstCell.click();
			await page.getByTestId('save-button').click();
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
			await expect(wantedProgress).toContainText('1 Doppelte');
			const wantedBar = wantedProgress.getByRole('progressbar');
			const wantedLabel = await wantedBar.getAttribute('aria-label');
			const wantedTotalMatch = wantedLabel?.match(/1 von (\d+) Heften/);
			expect(wantedTotalMatch).not.toBeNull();
			const wantedExpectedPercent = 100 / Number(wantedTotalMatch![1]);
			expect(Number(await wantedBar.getAttribute('aria-valuenow'))).toBeCloseTo(
				wantedExpectedPercent,
				8
			);

			await changeFirstEntryStatus('duplicate');
			await page.goto('/');
			const duplicateProgress = page.getByTestId('series-progress-bar').first();
			await expect(duplicateProgress).toContainText('2 Doppelte');
			const duplicateBar = duplicateProgress.getByRole('progressbar');
			const progressLabel = await duplicateBar.getAttribute('aria-label');
			const totalMatch = progressLabel?.match(/2 von (\d+) Heften/);
			expect(totalMatch).not.toBeNull();
			const expectedPercent = 200 / Number(totalMatch![1]);
			await expect(duplicateProgress).toContainText(`2 von ${totalMatch![1]}`);
			expect(Number(await duplicateBar.getAttribute('aria-valuenow'))).toBeCloseTo(
				expectedPercent,
				8
			);
		} finally {
			await changeFirstEntryStatus('owned');
		}
	});

	test('empty collection does not show progress for unrelated active series', async ({ page }) => {
		type SnapshotEntry = {
			id: number;
			issue_id: number;
			copy_number: number | null;
			condition_grade: string | null;
			status: 'owned' | 'duplicate' | 'wanted';
			notes: string | null;
		};

		const collectionResponse = await page.request.get('/api/v1/me/collection?per_page=100');
		expect(collectionResponse.ok()).toBe(true);
		const snapshot = (await collectionResponse.json()) as { data: SnapshotEntry[] };
		expect(snapshot.data.length).toBeGreaterThan(0);

		const deletedEntries: SnapshotEntry[] = [];
		try {
			for (const entry of snapshot.data) {
				const deleteResponse = await page.request.delete(`/api/v1/me/collection/${entry.id}`);
				if (deleteResponse.ok()) deletedEntries.push(entry);
				expect(deleteResponse.ok()).toBe(true);
			}

			const statsResponse = await page.request.get('/api/v1/me/collection/stats');
			expect(statsResponse.ok()).toBe(true);
			const stats = (await statsResponse.json()) as {
				total_issues: number | null;
				overall_progress_percent: number | null;
				series_stats: unknown[];
			};
			expect(stats.total_issues).toBeNull();
			expect(stats.overall_progress_percent).toBeNull();
			expect(stats.series_stats).toEqual([]);

			await page.goto('/');
			await expect(page.getByTestId('empty-state')).toBeVisible();
			await expect(page.getByTestId('series-progress-section')).toHaveCount(0);
		} finally {
			for (const entry of deletedEntries) {
				const restoreResponse = await page.request.post('/api/v1/me/collection', {
					data: {
						issue_id: entry.issue_id,
						copy_number: entry.copy_number ?? 1,
						condition_grade: entry.condition_grade ?? 'Z1',
						status: entry.status,
						notes: entry.notes
					}
				});
				expect(restoreResponse.ok()).toBe(true);
			}
		}
	});
});

// ---------------------------------------------------------------------------
// Unauthenticated access
// ---------------------------------------------------------------------------

unauthenticatedTest.describe('Collection – Unauthenticated', () => {
	unauthenticatedTest('redirects to login when not authenticated', async ({ page }) => {
		await page.goto('/collection');
		await expect(page).toHaveURL(/\/login/, { timeout: 10000 });
	});

	unauthenticatedTest('add page redirects to login when not authenticated', async ({ page }) => {
		await page.goto('/collection/add');
		await expect(page).toHaveURL(/\/login/, { timeout: 10000 });
	});
});
