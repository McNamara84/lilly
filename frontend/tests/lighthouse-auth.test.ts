import { createRequire } from 'node:module';
import { afterEach, describe, expect, it, vi } from 'vitest';

const require = createRequire(import.meta.url);
const authenticate = require('../scripts/lighthouse-auth.cjs') as (
	browser: { newPage: () => Promise<ReturnType<typeof createPage>> },
	context: { url: string }
) => Promise<void>;

function createPage(result = { ok: true, status: 200 }) {
	return {
		goto: vi.fn(async () => undefined),
		evaluate: vi.fn(async () => result),
		close: vi.fn(async () => undefined)
	};
}

afterEach(() => {
	vi.unstubAllEnvs();
});

describe('Lighthouse authentication', () => {
	it('passes configured credentials into the browser context', async () => {
		vi.stubEnv('LIGHTHOUSE_AUTH_EMAIL', 'lighthouse@example.test');
		vi.stubEnv('LIGHTHOUSE_AUTH_PASSWORD', 'rotated-password');
		const page = createPage();
		const browser = { newPage: vi.fn(async () => page) };

		await authenticate(browser, { url: 'https://lilly.test/collection' });

		expect(page.goto).toHaveBeenCalledWith('https://lilly.test', {
			waitUntil: 'domcontentloaded'
		});
		expect(page.evaluate).toHaveBeenCalledWith(expect.any(Function), {
			email: 'lighthouse@example.test',
			password: 'rotated-password'
		});
		expect(page.close).toHaveBeenCalledOnce();
	});

	it('uses local defaults and closes the page after a failed login', async () => {
		vi.stubEnv('LIGHTHOUSE_AUTH_EMAIL', '');
		vi.stubEnv('LIGHTHOUSE_AUTH_PASSWORD', '');
		const page = createPage({ ok: false, status: 401 });
		const browser = { newPage: vi.fn(async () => page) };

		await expect(authenticate(browser, { url: 'http://localhost:4173/' })).rejects.toThrow(
			'Lighthouse login failed with HTTP 401'
		);
		expect(page.evaluate).toHaveBeenCalledWith(expect.any(Function), {
			email: 'e2e-worker-0@lilly.app',
			password: 'e2e-worker-password'
		});
		expect(page.close).toHaveBeenCalledOnce();
	});
});
