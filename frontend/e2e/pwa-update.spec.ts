import { expect, unauthenticatedTest as test } from './fixtures';
import { waitForOfflineData, queueOfflineChanges } from './pwa-helpers';

test.skip(
	!process.env.PWA_UPDATE_CONTROL_TOKEN,
	'Run with npm run test:pwa:update to serve two production builds'
);
test.setTimeout(120_000);

test.beforeEach(async ({ page }, info) => {
	expect(
		(
			await page.request.post('/__pwa_test__/switch', {
				headers: { 'x-pwa-test-token': process.env.PWA_UPDATE_CONTROL_TOKEN! },
				data: { build: 'a' }
			})
		).ok()
	).toBe(true);
	expect(
		(
			await page.request.post('/api/v1/auth/login', {
				data: { email: `e2e-pwa-${info.project.name}-0@lilly.app`, password: 'e2e-pwa-password' }
			})
		).ok()
	).toBe(true);
	let previous = await (await page.request.get('/api/v1/me/collection?per_page=100')).json();
	while (previous.data.length) {
		for (const entry of previous.data)
			expect((await page.request.delete(`/api/v1/me/collection/${entry.id}`)).ok()).toBe(true);
		previous = await (await page.request.get('/api/v1/me/collection?per_page=100')).json();
	}
	const issues = await (await page.request.get('/api/v1/series/maddrax/issues?per_page=1')).json();
	expect(
		(
			await page.request.post('/api/v1/me/collection', {
				data: { issue_id: issues.data[0].id, notes: 'update fixture', condition_grade: 'Z1' }
			})
		).ok()
	).toBe(true);
	await page.goto('/collection');
	await expect(page.getByTestId('cover-card').first()).toBeVisible();
	await waitForOfflineData(page);
	await expect
		.poll(() => page.evaluate(async () => (await caches.keys()).includes('lilly-app-pwa-test-a')))
		.toBe(true);
});

test('updates from build A to B without losing queued data or breaking another open tab', async ({
	page,
	context
}) => {
	const second = await context.newPage();
	await second.goto('/collection');
	await expect(second.getByTestId('cover-card').first()).toBeVisible();
	await second.getByTestId('cover-card').first().click();
	await second.getByTestId('notes-textarea').fill('unsaved in old tab');
	await context.route('**/api/v1/me/collection/sync', (route) => route.abort('failed'));
	await queueOfflineChanges(page, 1, 'create', 'survives update ');
	await page.request.post('/__pwa_test__/switch', {
		headers: { 'x-pwa-test-token': process.env.PWA_UPDATE_CONTROL_TOKEN! },
		data: { build: 'b' }
	});
	await page.evaluate(async () => {
		await (await navigator.serviceWorker.ready).update();
	});
	await expect(page.getByTestId('pwa-update-notice')).toBeVisible({ timeout: 20000 });
	await page.getByRole('button', { name: 'Jetzt aktualisieren' }).click();
	await expect(page.getByTestId('collection-title')).toBeVisible();
	await expect(page.getByTestId('offline-status')).toContainText('ausstehend');
	await expect(second.getByTestId('notes-textarea')).toHaveValue('unsaved in old tab');
	expect(await page.evaluate(async () => caches.keys())).toContain('lilly-app-pwa-test-a');
	await context.unroute('**/api/v1/me/collection/sync');
	await page.getByTestId('offline-status').click();
	// Reload may interrupt a lease owner; another tab can reclaim it after at most 60 seconds.
	await expect(page.getByTestId('offline-status')).toHaveText('Synchronisiert', {
		timeout: 80_000
	});
	const entries = await (await page.request.get('/api/v1/me/collection?per_page=100')).json();
	expect(
		entries.data.filter((entry: { notes: string }) => entry.notes === 'survives update 0')
	).toHaveLength(1);
	await second.close();
	await page.evaluate(() => window.dispatchEvent(new Event('focus')));
	await expect
		.poll(() => page.evaluate(async () => (await caches.keys()).includes('lilly-app-pwa-test-a')))
		.toBe(false);
});

test('keeps build A usable after an interrupted update and allows retrying B', async ({ page }) => {
	await page.request.post('/__pwa_test__/switch', {
		headers: { 'x-pwa-test-token': process.env.PWA_UPDATE_CONTROL_TOKEN! },
		data: { build: 'b', failAssets: true }
	});
	await page.evaluate(async () => {
		const registration = await navigator.serviceWorker.ready;
		const failed = new Promise<void>((resolve) =>
			registration.addEventListener(
				'updatefound',
				() => {
					const installing = registration.installing!;
					installing.addEventListener('statechange', () => {
						if (installing.state === 'redundant') resolve();
					});
				},
				{ once: true }
			)
		);
		await registration.update();
		await failed;
	});
	await expect(page.getByTestId('cover-card').first()).toBeVisible();
	await expect(page.getByTestId('pwa-update-notice')).toHaveCount(0);
	await page.request.post('/__pwa_test__/switch', {
		headers: { 'x-pwa-test-token': process.env.PWA_UPDATE_CONTROL_TOKEN! },
		data: { build: 'b' }
	});
	await page.evaluate(async () => {
		await (await navigator.serviceWorker.ready).update();
	});
	await expect(page.getByTestId('pwa-update-notice')).toBeVisible({ timeout: 20000 });
	await page.getByRole('button', { name: 'Jetzt aktualisieren' }).click();
	await expect(page.getByTestId('collection-title')).toBeVisible();
});
