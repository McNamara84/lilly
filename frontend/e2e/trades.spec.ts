import AxeBuilder from '@axe-core/playwright';
import type { APIRequestContext, Page } from '@playwright/test';
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

			await page.goto('/trades/offers');
			const offer = page.getByTestId('offer-card').filter({ hasText: 'Der Gott aus dem Eis' });
			await expect(offer).toHaveCount(1);
			await expect(offer).toContainText('Der Gott aus dem Eis');
			await expect(offer).toContainText('Zustand');
			await offer.getByRole('button', { name: 'Nicht mehr tauschbar' }).click();
			await expect(offer).toHaveCount(0);

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

			await page.goto('/trades/wanted');
			const wanted = page.getByTestId('wanted-card').filter({ hasText: 'Der Gott aus dem Eis' });
			await expect(wanted).toHaveCount(1);
			await expect(wanted).toContainText('Der Gott aus dem Eis');
			await wanted.getByRole('button', { name: 'Entfernen' }).click();
			await expect(wanted).toHaveCount(0);
		} finally {
			await ensureOwned(page, issueId);
		}
	});

	test('match, proposal, acceptance and bidirectional messages form one complete flow', async ({
		page,
		playwright
	}, testInfo) => {
		let tradeId: number | undefined;
		let partner: APIRequestContext | undefined;
		try {
			await page.goto('/trades');
			const match = page
				.getByTestId('trade-match-card')
				.filter({ hasText: `E2E Partner ${testInfo.parallelIndex}` });
			await expect(match).toHaveCount(1);
			const proposalResponsePromise = page.waitForResponse(
				(response) =>
					response.request().method() === 'POST' &&
					/\/api\/v1\/me\/matches\/\d+\/proposals$/.test(new URL(response.url()).pathname)
			);
			await match.getByRole('button', { name: 'Tausch vorschlagen' }).click();
			const proposalResponse = await proposalResponsePromise;
			expect(proposalResponse.ok()).toBe(true);
			tradeId = ((await proposalResponse.json()) as { id: number }).id;
			expect(tradeId).toBeGreaterThan(0);
			const tradeLink = page.getByRole('link', { name: 'Details und Nachrichten' });
			await expect(tradeLink).toBeVisible();
			await Promise.all([page.waitForURL(/\/trades\/\d+\/?$/), tradeLink.click()]);
			await expect(page.getByRole('heading', { name: /Tausch mit E2E Partner/ })).toBeVisible();
			const tradePath = new URL(page.url()).pathname;
			const tradeIdMatch = tradePath.match(/^\/trades\/(\d+)\/?$/);
			expect(tradeIdMatch).not.toBeNull();
			expect(Number(tradeIdMatch![1])).toBe(tradeId);

			const baseURL = testInfo.project.use.baseURL;
			if (typeof baseURL !== 'string') throw new TypeError('E2E baseURL must be configured');
			partner = await playwright.request.newContext({ baseURL });
			const login = await partner.post('/api/v1/auth/login', {
				data: {
					email: `e2e-partner-${testInfo.parallelIndex}@lilly.app`,
					password: 'e2e-partner-password'
				}
			});
			expect(login.ok()).toBe(true);
			const accepted = await partner.post(`/api/v1/me/trades/${tradeId}/accept`);
			expect(accepted.ok()).toBe(true);
			const trade = (await accepted.json()) as { thread_id: number };
			const message = await partner.post(`/api/v1/me/messages/${trade.thread_id}`, {
				data: {
					client_message_id: crypto.randomUUID(),
					content: 'Ich versende gern als BüWa.'
				}
			});
			expect(message.ok()).toBe(true);

			await page.reload();
			await expect(page.getByText('Aktiv')).toBeVisible();
			await expect(page.getByText('Ich versende gern als BüWa.')).toBeVisible();
			await page.getByRole('textbox', { name: 'Nachricht' }).fill('Perfekt, danke!');
			await page.getByRole('button', { name: 'Senden' }).click();
			await expect(page.getByText('Perfekt, danke!')).toBeVisible();

			const history = await partner.get(`/api/v1/me/messages/${trade.thread_id}`);
			expect(history.ok()).toBe(true);
			expect(await history.text()).toContain('Perfekt, danke!');
		} finally {
			if (tradeId !== undefined) {
				const cancelled = await page.request.post(`/api/v1/me/trades/${tradeId}/cancel`);
				expect(cancelled.ok()).toBe(true);
			}
			await partner?.dispose();
		}
	});

	test('trade list pages have no automatically detectable accessibility violations', async ({
		page
	}) => {
		await page.goto('/trades');
		await expect(page.getByRole('heading', { level: 1, name: 'Tausch' })).toBeVisible();
		let results = await new AxeBuilder({ page }).analyze();
		expect(results.violations).toEqual([]);

		await page.goto('/trades/wanted/add');
		await expect(page.getByTestId('series-select')).toBeVisible();
		results = await new AxeBuilder({ page }).analyze();
		expect(results.violations).toEqual([]);
	});
});
