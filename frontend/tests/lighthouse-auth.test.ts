import { createRequire } from 'node:module';
import { afterEach, describe, expect, it, vi } from 'vitest';
const require = createRequire(import.meta.url);
const { authenticate, sessionCookies } = require('../scripts/lighthouse-auth.cjs');
const { median, summarize, validateReport, auditOnce } = require('../scripts/lighthouse-run.cjs');

afterEach(() => vi.unstubAllEnvs());

function report(authenticated = false, fcp = 1000, performance = 0.95) {
	return {
		finalDisplayedUrl: `https://lilly.test/${authenticated ? 'collection' : 'login'}`,
		audits: { 'first-contentful-paint': { numericValue: fcp } },
		categories: { performance: { score: performance } }
	};
}

describe('cold Lighthouse measurement', () => {
	it('authenticates through HTTP without visiting the app and imports only the session', async () => {
		vi.stubEnv('LIGHTHOUSE_AUTH_EMAIL', 'fixture@example.test');
		vi.stubEnv('LIGHTHOUSE_AUTH_PASSWORD', 'test-password');
		const page = { setCookie: vi.fn() };
		const request = vi.fn(async (_url: string, _init: RequestInit) => ({
			ok: true,
			headers: { getSetCookie: () => ['session=secret==; Path=/; HttpOnly; Secure; SameSite=Lax'] }
		}));
		await authenticate(page, 'https://lilly.test', request);
		expect(JSON.parse(String(request.mock.calls[0][1].body))).toEqual({
			email: 'fixture@example.test',
			password: 'test-password'
		});
		expect(page.setCookie).toHaveBeenCalledWith({
			name: 'session',
			value: 'secret==',
			url: 'https://lilly.test',
			httpOnly: true,
			secure: true,
			sameSite: 'Lax'
		});
	});

	it('rejects failed login, absent cookies and malformed cookie data', async () => {
		await expect(
			authenticate({}, 'https://lilly.test', async () => ({ ok: false, status: 401 }))
		).rejects.toThrow('401');
		await expect(
			authenticate({}, 'https://lilly.test', async () => ({
				ok: true,
				headers: { getSetCookie: () => [] }
			}))
		).rejects.toThrow('no session');
		expect(() => sessionCookies({ getSetCookie: () => ['invalid'] }, 'https://lilly.test')).toThrow(
			'Invalid'
		);
	});

	it('uses exactly five finite values and evaluates each route independently', () => {
		expect(median([5, 1, 4, 2, 3])).toBe(3);
		expect(() => median([1])).toThrow('five');
		expect(() => median([1, 2, 3, 4, NaN])).toThrow('finite');
		const runs = ['public', 'collection'].flatMap((route) =>
			Array.from({ length: 5 }, () => ({ route, fcp: 3000, performance: 0.9 }))
		);
		expect(summarize(runs).passed).toBe(true);
		for (const run of runs.filter((run) => run.route === 'collection')) run.performance = 0.89;
		expect(summarize(runs).passed).toBe(false);
		for (const run of runs) {
			run.performance = 1;
			run.fcp = 3001;
		}
		expect(summarize(runs).passed).toBe(false);
	});

	it('rejects errors, missing measurements and the wrong final route', () => {
		expect(validateReport(report(), false)).toEqual({ fcp: 1000, performance: 0.95 });
		expect(() => validateReport(undefined, false)).toThrow('valid navigation');
		expect(() => validateReport({ runtimeError: {} }, false)).toThrow('valid navigation');
		expect(() => validateReport(report(true), false)).toThrow('Unexpected');
		expect(() => validateReport(report(false, NaN), false)).toThrow('missing');
	});

	it.each([false, true])(
		'starts and closes a fresh browser for every run (authenticated=%s)',
		async (authenticated) => {
			const page = {
				on: vi.fn(),
				bringToFront: vi.fn(),
				setCookie: vi.fn(),
				waitForSelector: vi.fn(),
				waitForFunction: vi.fn(),
				evaluate: vi.fn(async () => ({ valid: true, total: authenticated ? 200 : 0 }))
			};
			const extraTab = { close: vi.fn() };
			const browser = {
				newPage: vi.fn(async () => page),
				pages: vi.fn(async () => [extraTab, page]),
				close: vi.fn()
			};
			const dependencies = {
				launch: vi.fn(async () => browser),
				saveReport: vi.fn(),
				audit: vi.fn(async (_url: string) => ({
					lhr: report(authenticated),
					report: ['', '<html>secret-token</html>']
				})),
				request: vi.fn(async () => ({
					ok: true,
					headers: { getSetCookie: () => ['session=secret-token'] }
				}))
			};
			const result = await auditOnce('https://lilly.test', authenticated, dependencies);
			await auditOnce('https://lilly.test', authenticated, dependencies);
			expect(dependencies.launch).toHaveBeenCalledTimes(2);
			expect(browser.close).toHaveBeenCalledTimes(2);
			expect(extraTab.close).toHaveBeenCalledTimes(2);
			expect(page.bringToFront).toHaveBeenCalledTimes(2);
			expect(dependencies.audit.mock.calls[0][0]).toBe(
				`https://lilly.test${authenticated ? '/collection' : '/'}`
			);
			expect(dependencies.request).toHaveBeenCalledTimes(authenticated ? 2 : 0);
			if (authenticated) expect(result.html).not.toContain('secret-token');
			page.evaluate.mockResolvedValueOnce({ valid: false, total: 0 });
			await expect(auditOnce('https://lilly.test', authenticated, dependencies)).rejects.toThrow(
				'Invalid page'
			);
			expect(browser.close).toHaveBeenCalledTimes(3);
			dependencies.audit.mockResolvedValueOnce({
				lhr: Object.assign(report(authenticated), { runtimeError: { code: 'NO_NAVSTART' } }),
				report: ['', '<html>secret-token</html>']
			});
			await expect(auditOnce('https://lilly.test', authenticated, dependencies)).rejects.toThrow(
				'NO_NAVSTART'
			);
			expect(dependencies.saveReport).toHaveBeenCalledTimes(4);
			const invalid = dependencies.saveReport.mock.lastCall![0];
			expect(JSON.parse(invalid.json).runtimeError.code).toBe('NO_NAVSTART');
			if (authenticated) expect(invalid.html).not.toContain('secret-token');
			expect(browser.close).toHaveBeenCalledTimes(4);
		}
	);
});
