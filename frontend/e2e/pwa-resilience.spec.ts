import { expect, unauthenticatedTest as test } from './fixtures';
import { waitForOfflineData, queueOfflineChanges } from './pwa-helpers';

test.beforeEach(async ({ page }, info) => {
	const email = `e2e-pwa-${info.project.name}-${info.parallelIndex}@lilly.app`;
	const login = await page.request.post('/api/v1/auth/login', {
		data: { email, password: 'e2e-pwa-password' }
	});
	expect(login.ok()).toBe(true);
	const previous = await page.request.get('/api/v1/me/collection?per_page=100');
	let body = await previous.json();
	while (body.data.length) {
		for (const entry of body.data)
			expect((await page.request.delete(`/api/v1/me/collection/${entry.id}`)).ok()).toBe(true);
		body = await (await page.request.get('/api/v1/me/collection?per_page=100')).json();
	}
	const issues = await (
		await page.request.get('/api/v1/series/maddrax/issues?per_page=100')
	).json();
	// Keep bulk queue tests independent of trade fixtures and other browser workers.
	const number =
		80 +
		['chromium', 'firefox', 'webkit', 'mobile-chrome'].indexOf(info.project.name) * 4 +
		info.parallelIndex;
	const issue = issues.data.find((item: { issue_number: number }) => item.issue_number === number);
	expect(issue).toBeDefined();
	expect(
		(
			await page.request.post('/api/v1/me/collection', {
				data: {
					issue_id: issue.id,
					status: 'owned',
					condition_grade: 'Z2',
					notes: 'baseline'
				}
			})
		).ok()
	).toBe(true);
	await page.goto('/collection');
	await expect(page.getByTestId('cover-card').first()).toBeVisible();
	await waitForOfflineData(page);
});

test('synchronizes more than one hundred durable changes in bounded requests', async ({
	page,
	context,
	browserName
}) => {
	const sizes: number[] = [];
	page.on('request', (request) => {
		if (request.url().endsWith('/collection/sync'))
			sizes.push(request.postDataJSON().mutations.length);
	});
	await context.setOffline(true);
	await queueOfflineChanges(page, 101, 'create', 'batched ');
	if (browserName !== 'webkit') await page.reload();
	await expect(page.getByTestId('offline-status')).toContainText('101');
	await context.setOffline(false);
	await expect(page.getByTestId('offline-status')).toHaveText('Synchronisiert', { timeout: 20000 });
	const result = await (await page.request.get('/api/v1/me/collection?per_page=100')).json();
	expect(result.total).toBe(102);
	expect(sizes).toContain(100);
	expect(sizes).toContain(1);
	expect(Math.max(...sizes)).toBeLessThanOrEqual(100);
});

test('retries a committed request after its response is lost without creating duplicates', async ({
	page,
	context
}) => {
	// WebKit does not reliably route service-worker-controlled requests through page.route.
	// Drop the real response at the fetch boundary, after the server has committed it.
	await page.evaluate(() => {
		const target = window as Window & { syncIds?: string[][] };
		target.syncIds = [];
		const originalFetch = window.fetch.bind(window);
		window.fetch = async (input, init) => {
			if (!String(input).endsWith('/collection/sync')) return originalFetch(input, init);
			const ids = target.syncIds!;
			ids.push(
				JSON.parse(String(init?.body)).mutations.map(
					(item: { mutation_id: string }) => item.mutation_id
				)
			);
			const response = await originalFetch(input, init);
			if (ids.length === 1) throw new TypeError('Test fixture: response lost after server commit');
			return response;
		};
	});
	await context.setOffline(true);
	await queueOfflineChanges(page, 1, 'create', 'lost response ');
	await context.setOffline(false);
	await expect
		.poll(
			() => page.evaluate(() => (window as unknown as { syncIds: string[][] }).syncIds.length),
			{
				timeout: 15000
			}
		)
		.toBeGreaterThanOrEqual(1);
	await expect(page.getByTestId('offline-status')).toContainText('ausstehend');
	await page.getByTestId('offline-status').click();
	await expect(page.getByTestId('offline-status')).toHaveText('Synchronisiert');
	const ids = await page.evaluate(() => (window as unknown as { syncIds: string[][] }).syncIds);
	expect(ids).toHaveLength(2);
	expect(ids[1]).toEqual(ids[0]);
	expect((await (await page.request.get('/api/v1/me/collection')).json()).total).toBe(2);
});

for (const decision of ['server', 'local'] as const) {
	test(`keeps a conflict until the user explicitly chooses the ${decision} version`, async ({
		page,
		context
	}) => {
		await context.setOffline(true);
		await queueOfflineChanges(page, 1, 'update', 'local ');
		const collection = await (await page.request.get('/api/v1/me/collection')).json();
		const entry = collection.data[0];
		expect(
			(
				await page.request.patch(`/api/v1/me/collection/${entry.id}`, {
					data: { notes: 'server version', base_revision: entry.revision }
				})
			).ok()
		).toBe(true);
		await context.setOffline(false);
		await expect(page.getByTestId('offline-status')).toContainText('Konflikt');
		await page.getByTestId('offline-status').click();
		await expect(page.getByTestId('conflict-panel')).toBeVisible();
		await page
			.getByRole('button', {
				name: decision === 'server' ? 'Serverstand übernehmen' : 'Lokalen Stand erneut anwenden'
			})
			.click();
		await expect(page.getByTestId('offline-status')).toHaveText('Synchronisiert');
		const saved = await (await page.request.get('/api/v1/me/collection')).json();
		expect(saved.data[0].notes).toBe(decision === 'server' ? 'server version' : 'local 0');
	});
}

test('removes private data in both tabs after logout and isolates the next account', async ({
	page,
	context,
	browserName
}) => {
	test.setTimeout(60000);
	const second = await context.newPage();
	await second.goto('/collection');
	await expect(second.getByTestId('cover-card').first()).toBeVisible();
	await page.getByTestId('header-logout-button').click();
	await expect(page.getByTestId('login-form')).toBeVisible();
	await expect(second.getByTestId('cover-card')).toHaveCount(0);
	await expect(second.getByTestId('login-form')).toBeVisible();
	await context.setOffline(true);
	if (browserName !== 'webkit') await second.reload();
	await expect(second.getByTestId('cover-card')).toHaveCount(0);
	await context.setOffline(false);
	await second.close();
	// Sign in through the real form as another user, without resetting browser storage.
	await page.getByTestId('email-input').fill('e2e-performance@lilly.app');
	await page.getByTestId('password-input').fill('e2e-pwa-password');
	await page.getByTestId('submit-button').click();
	await expect(page.getByTestId('header-logout-button')).toBeVisible();
	const nextUser = await (await page.request.get('/api/v1/auth/me')).json();
	expect(nextUser.email).toBe('e2e-performance@lilly.app');
	await page.goto('/collection');
	await expect(page.getByTestId('cover-card').first()).toBeVisible();
	await expect
		.poll(
			() =>
				page.evaluate(
					() =>
						new Promise<{ users: number[]; count: number }>((resolve, reject) => {
							const open = indexedDB.open('lilly-offline');
							open.onerror = () => reject(open.error);
							open.onsuccess = () => {
								const db = open.result;
								const tx = db.transaction('collection_entries');
								const all = tx.objectStore('collection_entries').getAll();
								all.onsuccess = () =>
									resolve({
										users: [...new Set<number>(all.result.map((entry) => entry.user_id))],
										count: all.result.length
									});
								tx.oncomplete = () => db.close();
							};
						})
				),
			// The new account writes 200 collection entries plus its catalog for the first time.
			{ timeout: 15000 }
		)
		.toEqual({ users: [nextUser.id], count: 200 });
	await context.setOffline(true);
	if (browserName !== 'webkit') await page.reload();
	await expect(page.getByTestId('cover-card').first()).toBeVisible();
	await context.setOffline(false);
});

test('reads both initial series offline', async ({ page, context }) => {
	await page.goto('/collection/add');
	await expect(page.getByTestId('series-card')).toHaveCount(2);
	await context.setOffline(true);
	for (const name of ['Maddrax', 'John Sinclair']) {
		await page.getByTestId('series-card').filter({ hasText: name }).click();
		await expect(page.getByTestId('series-status-grid')).toBeVisible();
		await page.getByTestId('back-button').click();
		await expect(page.getByTestId('series-card')).toHaveCount(2);
	}
	await context.setOffline(false);
});

test('clears the failed logout alert only after signing in successfully again', async ({
	page
}, info) => {
	// Inject at fetch so this also reaches service-worker-controlled WebKit pages.
	await page.evaluate(() => {
		const originalFetch = window.fetch.bind(window);
		window.fetch = async (input, init) => {
			if (!String(input).endsWith('/auth/logout')) return originalFetch(input, init);
			window.fetch = originalFetch;
			return new Response(JSON.stringify({ error: 'Injected logout failure' }), {
				status: 503,
				headers: { 'Content-Type': 'application/json' }
			});
		};
	});
	await page.getByTestId('header-logout-button').click();
	await expect(page.getByTestId('login-form')).toBeVisible();
	const alert = page.getByRole('alert').filter({ hasText: 'Abmeldung beim Server' });
	await expect(alert).toBeVisible();
	await page
		.getByTestId('email-input')
		.fill(`e2e-pwa-${info.project.name}-${info.parallelIndex}@lilly.app`);
	await page.getByTestId('password-input').fill('incorrect-password');
	await page.getByTestId('submit-button').click();
	await expect(page.getByTestId('login-error')).toBeVisible();
	await expect(alert).toBeVisible();
	await page.getByTestId('password-input').fill('e2e-pwa-password');
	await page.getByTestId('submit-button').click();
	await expect(page.getByTestId('header-logout-button')).toBeVisible();
	await expect(alert).toHaveCount(0);
});

test('ignores delayed session-end notifications after the same account signs in again', async ({
	page,
	context,
	browserName
}, info) => {
	const account = await (await page.request.get('/api/v1/auth/me')).json();
	const previousVersion = await page.evaluate(() => localStorage.getItem('lilly-offline-session'));
	await page.getByTestId('header-logout-button').click();
	await expect(page.getByTestId('login-form')).toBeVisible();
	const endedVersion = await page.evaluate(() => localStorage.getItem('lilly-offline-session'));
	await page
		.getByTestId('email-input')
		.fill(`e2e-pwa-${info.project.name}-${info.parallelIndex}@lilly.app`);
	await page.getByTestId('password-input').fill('e2e-pwa-password');
	await page.getByTestId('submit-button').click();
	await expect(page.getByTestId('header-logout-button')).toBeVisible();
	await page.goto('/collection');
	await expect(page.getByTestId('cover-card').first()).toBeVisible();
	await waitForOfflineData(page);
	const freshVersion = await page.evaluate(() => localStorage.getItem('lilly-offline-session'));
	expect(freshVersion).not.toBe(previousVersion);
	await page.evaluate(
		async ({ userId, previousVersion, endedVersion }) => {
			const sender = new BroadcastChannel('lilly-offline');
			const observer = new BroadcastChannel('lilly-offline');
			// Wait for delivery to all previously created receivers, including the auth listener.
			const delivered = new Promise<void>((resolve) => {
				observer.onmessage = (event) => {
					if (event.data.type === 'account-deletion') resolve();
				};
			});
			for (const type of ['logout', 'account-deletion']) {
				sender.postMessage({
					type,
					user_id: userId,
					previous_version: previousVersion,
					session_version: endedVersion
				});
			}
			await delivered;
			sender.close();
			observer.close();
		},
		{ userId: account.id, previousVersion, endedVersion }
	);
	expect(await page.evaluate(() => localStorage.getItem('lilly-offline-session'))).toBe(
		freshVersion
	);
	await expect(page.getByTestId('header-logout-button')).toBeVisible();
	await waitForOfflineData(page);
	await context.setOffline(true);
	if (browserName !== 'webkit') await page.reload();
	await expect(page.getByTestId('cover-card')).toHaveCount(1);
	await expect(page.getByTestId('header-logout-button')).toBeVisible();
	await context.setOffline(false);
});

test('creates and edits both series offline through every condition grade', async ({
	page,
	context
}) => {
	test.setTimeout(60000);
	await page.goto('/collection/add');
	await expect(page.getByTestId('series-card')).toHaveCount(2);
	await context.setOffline(true);
	for (const name of ['Maddrax', 'John Sinclair']) {
		await page.getByTestId('series-card').filter({ hasText: name }).click();
		const missing = page
			.locator('[data-testid="series-status-cell"][data-status="missing"]')
			.first();
		await expect(missing).toBeVisible();
		const position = await missing.evaluate((element) =>
			Array.from(element.parentElement!.children).indexOf(element)
		);
		for (let grade = 0; grade <= 4; grade++) {
			await page.getByTestId('series-status-cell').nth(position).click();
			await page.getByTestId('status-owned').click();
			await page.getByTestId(`condition-chip-Z${grade}`).click();
			await page.getByTestId('notes-textarea').fill(`offline grades ${name} Z${grade}`);
			await page.getByTestId('save-button').click();
			await expect(page.getByTestId('issue-detail-sheet')).toBeHidden();
		}
		await page.getByTestId('back-button').click();
	}
	await expect(page.getByTestId('offline-status')).toContainText('ausstehend');
	await context.setOffline(false);
	await expect(page.getByTestId('offline-status')).toHaveText('Synchronisiert', { timeout: 20000 });
	const result = await (await page.request.get('/api/v1/me/collection')).json();
	const edited = result.data.filter((entry: { notes: string }) =>
		entry.notes?.startsWith('offline grades')
	);
	expect(edited).toHaveLength(2);
	expect(new Set(edited.map((entry: { series_slug: string }) => entry.series_slug))).toEqual(
		new Set(['maddrax', 'john-sinclair'])
	);
	for (const entry of edited) expect(entry.condition_grade).toBe('Z4');
});
