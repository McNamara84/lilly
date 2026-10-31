import { mkdir } from 'node:fs/promises';
import path from 'node:path';
import { test as base, expect, type APIRequestContext } from '@playwright/test';
import type { PlaywrightWorkerArgs } from '@playwright/test';
import { validateApiPayload } from './api-contract';

type TestFixtures = {
	anonymousRequest: APIRequestContext;
	apiContract: void;
};

type WorkerFixtures = {
	workerStorageState: string;
};

function projectBaseURL(baseURL: unknown): string {
	if (typeof baseURL !== 'string') {
		throw new TypeError('The Playwright project must define a string baseURL');
	}
	return baseURL;
}

async function authenticateWorker(
	playwright: PlaywrightWorkerArgs['playwright'],
	baseURL: string,
	parallelIndex: number,
	storageStatePath: string
) {
	const request = await playwright.request.newContext({ baseURL });
	try {
		const response = await request.post('/api/v1/auth/login', {
			data: {
				email: `e2e-worker-${parallelIndex}@lilly.app`,
				password: 'e2e-worker-password'
			}
		});
		if (!response.ok()) {
			throw new Error(`E2E worker login failed with status ${response.status()}`);
		}
		await request.storageState({ path: storageStatePath });
	} finally {
		await request.dispose();
	}
}

export const test = base.extend<TestFixtures, WorkerFixtures>({
	apiContract: [
		async ({ page }, use) => {
			const errors: string[] = [];
			await page.exposeBinding(
				'__lillyValidateApi',
				(
					_source,
					result: {
						url: string;
						method: string;
						status: number;
						contentType: string;
						body?: unknown;
						invalidJson: boolean;
					}
				) => {
					try {
						if (result.invalidJson) throw new Error('API returned invalid JSON');
						validateApiPayload(
							result.url,
							result.method,
							result.status,
							result.contentType,
							result.body
						);
					} catch (error) {
						errors.push(String(error));
					}
				}
			);
			await page.addInitScript(() => {
				const originalFetch = window.fetch.bind(window);
				window.fetch = async (input, init) => {
					const response = await originalFetch(input, init);
					const url = new URL(input instanceof Request ? input.url : String(input), location.href);
					if (
						url.origin !== location.origin ||
						!url.pathname.startsWith('/api/v1/') ||
						url.pathname.startsWith('/api/v1/docs') ||
						url.pathname === '/api/v1/openapi.json'
					)
						return response;
					// Clone at the fetch boundary: Chromium cannot always retrieve a service-worker
					// response body later through its debugging protocol, especially after navigation.
					const text = await response.clone().text();
					const contentType = response.headers.get('content-type') ?? '';
					let body: unknown = text || undefined;
					let invalidJson = false;
					if (text && contentType.startsWith('application/json')) {
						try {
							body = JSON.parse(text);
						} catch {
							invalidJson = true;
						}
					}
					const validate = (
						window as unknown as { __lillyValidateApi: (result: unknown) => Promise<void> }
					).__lillyValidateApi;
					await validate({
						url: url.href,
						method: init?.method ?? (input instanceof Request ? input.method : 'GET'),
						status: response.status,
						contentType,
						body,
						invalidJson
					});
					return response;
				};
			});
			await use();
			expect(errors, 'All completed browser API responses must match OpenAPI').toEqual([]);
		},
		{ auto: true }
	],
	storageState: async ({ workerStorageState }, use) => {
		await use(workerStorageState);
	},
	workerStorageState: [
		async ({ playwright }, use, workerInfo) => {
			const authDirectory = path.resolve(workerInfo.project.outputDir, '.auth');
			const storageStatePath = path.join(
				authDirectory,
				`${workerInfo.project.name}-${workerInfo.parallelIndex}.json`
			);
			await mkdir(authDirectory, { recursive: true });
			await authenticateWorker(
				playwright,
				projectBaseURL(workerInfo.project.use.baseURL),
				workerInfo.parallelIndex,
				storageStatePath
			);
			await use(storageStatePath);
		},
		{ scope: 'worker' }
	],
	anonymousRequest: async ({ playwright }, use, testInfo) => {
		const request = await playwright.request.newContext({
			baseURL: projectBaseURL(testInfo.project.use.baseURL),
			storageState: { cookies: [], origins: [] }
		});
		await use(request);
		await request.dispose();
	}
});

export const unauthenticatedTest = base;
export { expect };
