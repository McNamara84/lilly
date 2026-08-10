import { mkdir } from 'node:fs/promises';
import path from 'node:path';
import { test as base, expect, type APIRequestContext, type Playwright } from '@playwright/test';

type TestFixtures = {
	anonymousRequest: APIRequestContext;
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
	playwright: Playwright,
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
			baseURL: projectBaseURL(testInfo.project.use.baseURL)
		});
		await use(request);
		await request.dispose();
	}
});

export const unauthenticatedTest = base;
export { expect };
