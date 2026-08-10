import AxeBuilder from '@axe-core/playwright';
import type { Page } from '@playwright/test';
import { expect, test } from './fixtures';

async function firstMaddraxIssueId(page: Page): Promise<number> {
	return page.evaluate(async () => {
		const response = await fetch('/api/v1/series/maddrax/issues?page=1&per_page=1');
		if (!response.ok) throw new Error('Could not load the demo issue');
		const body = (await response.json()) as { data: Array<{ id: number }> };
		if (!body.data[0]) throw new Error('The demo issue is missing');
		return body.data[0].id;
	});
}

async function ensureOwned(page: Page, issueId: number) {
	await page.evaluate(async (id) => {
		const currentResponse = await fetch(`/api/v1/me/collection/by-issue/${id}`);
		if (!currentResponse.ok) throw new Error('Could not inspect the demo collection entry');
		const current = (await currentResponse.json()) as { id: number } | null;
		const response = current
			? await fetch(`/api/v1/me/collection/${current.id}`, {
					method: 'PATCH',
					headers: { 'Content-Type': 'application/json' },
					body: JSON.stringify({ status: 'owned', condition_grade: 'Z2' })
				})
			: await fetch('/api/v1/me/collection', {
					method: 'POST',
					headers: { 'Content-Type': 'application/json' },
					body: JSON.stringify({ issue_id: id, status: 'owned', condition_grade: 'Z2' })
				});
		if (!response.ok) throw new Error('Could not restore the demo collection entry');
	}, issueId);
}

async function removeCollectionEntry(page: Page, issueId: number) {
	await page.evaluate(async (id) => {
		const currentResponse = await fetch(`/api/v1/me/collection/by-issue/${id}`);
		if (!currentResponse.ok) throw new Error('Could not inspect the demo collection entry');
		const current = (await currentResponse.json()) as { id: number } | null;
		if (!current) return;
		const response = await fetch(`/api/v1/me/collection/${current.id}`, { method: 'DELETE' });
		if (!response.ok) throw new Error('Could not remove the demo collection entry');
	}, issueId);
}

test.describe('Trade lists', () => {
	test.beforeEach(async ({ page }) => {
		await page.goto('/');
	});

	test('duplicate offer and wanted issue workflows stay synchronized with the collection', async ({
		page
	}) => {
		const issueId = await firstMaddraxIssueId(page);
		await ensureOwned(page, issueId);

		try {
			await page.goto('/collection/add');
			await expect(page.getByTestId('loading-indicator')).toBeHidden({ timeout: 10000 });
			await page.getByTestId('series-card').first().click();
			await expect(page.getByTestId('series-status-cell').first()).toBeVisible();
			await page.getByTestId('series-status-cell').first().click();
			await page.getByTestId('status-duplicate').click();
			await page.getByTestId('save-button').click();
			await expect(page.getByTestId('issue-detail-sheet')).toBeHidden({ timeout: 5000 });

			await page.goto('/trades');
			const offer = page.getByTestId('offer-card').filter({ hasText: 'Der Gott aus dem Eis' });
			await expect(offer).toHaveCount(1);
			await expect(offer).toContainText('Der Gott aus dem Eis');
			await expect(offer).toContainText('Zustand');
			await offer.getByRole('button', { name: 'Nicht mehr tauschbar' }).click();
			await expect(page.getByTestId('offers-empty')).toBeVisible();

			await removeCollectionEntry(page, issueId);
			await page.goto('/trades/wanted/add');
			await expect(page.getByTestId('series-select')).toBeVisible();
			await page.getByTestId('series-select').selectOption('maddrax');
			const candidate = page
				.getByTestId('candidate-item')
				.filter({ hasText: 'Der Gott aus dem Eis' });
			await expect(candidate).toHaveCount(1);
			await expect(candidate).toContainText('Der Gott aus dem Eis');
			await candidate.getByRole('checkbox').check();
			await page.getByTestId('add-selection').click();
			await expect(candidate.getByRole('checkbox')).toBeDisabled();

			await page.goto('/trades');
			await page.getByTestId('wanted-tab').click();
			const wanted = page.getByTestId('wanted-card').filter({ hasText: 'Der Gott aus dem Eis' });
			await expect(wanted).toHaveCount(1);
			await expect(wanted).toContainText('Der Gott aus dem Eis');
			await wanted.getByRole('link', { name: 'Als vorhanden markieren' }).click();

			await expect(page.getByTestId('issue-detail-collection-panel')).toBeVisible();
			await page.getByRole('radio', { name: 'Vorhanden' }).click();
			await expect(page.getByTestId('condition-chips')).toBeVisible();
			await page.getByTestId('condition-chip-Z2').click();
			await page.getByRole('button', { name: 'Speichern' }).click();

			await page.goto('/trades');
			await page.getByTestId('wanted-tab').click();
			await expect(page.getByTestId('wanted-empty')).toBeVisible();
		} finally {
			await ensureOwned(page, issueId);
		}
	});

	test('trade list pages have no automatically detectable accessibility violations', async ({
		page
	}) => {
		await page.goto('/trades');
		await expect(page.getByRole('heading', { level: 1, name: 'Tauschlisten' })).toBeVisible();
		let results = await new AxeBuilder({ page }).analyze();
		expect(results.violations).toEqual([]);

		await page.goto('/trades/wanted/add');
		await expect(page.getByTestId('series-select')).toBeVisible();
		results = await new AxeBuilder({ page }).analyze();
		expect(results.violations).toEqual([]);
	});
});
