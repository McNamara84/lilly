import { expect, test } from './fixtures';
import { assertApiResponse } from './api-contract';

test('public, private and admin API responses match the published contract', async ({
	request,
	anonymousRequest
}) => {
	for (const path of [
		'/api/v1/health',
		'/api/v1/auth/options',
		'/api/v1/series',
		'/api/v1/media/photo-policy'
	]) {
		const response = await anonymousRequest.get(path);
		expect(response.status()).toBe(200);
		await assertApiResponse(response);
	}
	for (const path of [
		'/api/v1/auth/me',
		'/api/v1/me/collection',
		'/api/v1/me/trades',
		'/api/v1/me/notifications',
		'/api/v1/admin/account-erasure-jobs'
	]) {
		const denied = await anonymousRequest.get(path);
		expect(denied.status()).toBe(401);
		await assertApiResponse(denied);
		const allowed = await request.get(path);
		expect(allowed.status()).toBe(200);
		await assertApiResponse(allowed);
	}
	const malformed = await request.post('/api/v1/me/collection', { data: { issue_id: 'invalid' } });
	expect(malformed.status()).toBe(422);
	await assertApiResponse(malformed, 'POST');
});

test('Swagger UI loads local assets and executes an authenticated request under CSP', async ({
	page,
	baseURL
}) => {
	const externalRequests: string[] = [];
	const cspErrors: string[] = [];
	page.on('request', (request) => {
		if (new URL(request.url()).origin !== new URL(baseURL!).origin)
			externalRequests.push(request.url());
	});
	page.on('console', (message) => {
		if (/content security policy|violates.*directive/i.test(message.text()))
			cspErrors.push(message.text());
	});
	await page.goto('/api/v1/docs/');
	await expect(page.getByRole('heading', { name: /LILLY API/ })).toBeVisible();
	const operation = page.locator('#operations-auth-auth_me');
	await operation.locator('.opblock-summary').click();
	await operation.getByRole('button', { name: 'Try it out' }).click();
	const responsePromise = page.waitForResponse(
		(response) => new URL(response.url()).pathname === '/api/v1/auth/me'
	);
	await operation.getByRole('button', { name: 'Execute', exact: true }).click();
	const response = await responsePromise;
	expect(response.status()).toBe(200);
	await assertApiResponse(response);
	expect(externalRequests).toEqual([]);
	expect(cspErrors).toEqual([]);
});
