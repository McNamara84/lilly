/* eslint-disable @typescript-eslint/no-require-imports, no-console -- Node CLI entrypoint. */
const fs = require('node:fs/promises');
const path = require('node:path');
const { execFileSync } = require('node:child_process');
const { authenticate } = require('./lighthouse-auth.cjs');
const config = require('../lighthouserc.cjs');

function median(values) {
	if (values.length !== 5 || values.some((value) => !Number.isFinite(value)))
		throw new Error('Exactly five finite measurements are required');
	return [...values].sort((a, b) => a - b)[2];
}

function summarize(runs) {
	const results = ['public', 'collection'].map((route) => {
		const selected = runs.filter((run) => run.route === route);
		const fcp = median(selected.map((run) => run.fcp));
		const performance = median(selected.map((run) => run.performance));
		return { route, fcp, performance, passed: fcp <= 3000 && performance >= 0.9 };
	});
	return { results, passed: results.every((result) => result.passed) };
}

function validateReport(lhr, authenticated) {
	if (!lhr || lhr.runtimeError)
		throw new Error(
			`Lighthouse did not produce a valid navigation: ${lhr?.runtimeError?.code ?? 'no report'}`
		);
	const finalPath = new URL(lhr.finalDisplayedUrl || lhr.finalUrl).pathname;
	if (finalPath !== (authenticated ? '/collection' : '/login'))
		throw new Error(`Unexpected final route: ${finalPath}`);
	const fcp = lhr.audits['first-contentful-paint']?.numericValue;
	const performance = lhr.categories.performance?.score;
	if (!Number.isFinite(fcp) || !Number.isFinite(performance))
		throw new Error('Lighthouse metrics are missing');
	return { fcp, performance };
}

async function auditOnce(origin, authenticated, dependencies) {
	// A new browser process creates a fresh temporary profile for EACH measurement.
	const browser = await dependencies.launch();
	try {
		const page = await browser.newPage();
		// Measure exactly one root frame, excluding Chromium's additional startup tab.
		for (const other of await browser.pages()) if (other !== page) await other.close();
		await page.bringToFront();
		const errors = [];
		page.on('pageerror', (error) => errors.push(error.message));
		const cookies = authenticated ? await authenticate(page, origin, dependencies.request) : [];
		const url = `${origin}${authenticated ? '/collection' : '/'}`;
		const result = await dependencies.audit(
			url,
			{
				...config.ci.collect.settings,
				output: ['json', 'html'],
				logLevel: 'error',
				onlyCategories: ['performance'],
				disableStorageReset: false
			},
			undefined,
			page
		);
		const secrets = cookies.map((cookie) => cookie.value).filter(Boolean);
		const redact = (text) =>
			secrets.reduce((value, secret) => value.replaceAll(secret, '[redacted]'), text);
		const reports = {
			json: redact(JSON.stringify(result?.lhr ?? {})),
			html: redact(result?.report?.[1] ?? '')
		};
		// Preserve invalid reports too, so a missing navigation/metric remains diagnosable.
		await dependencies.saveReport?.(reports);
		const metrics = validateReport(result?.lhr, authenticated);
		await page.waitForSelector(
			authenticated ? '[data-testid="cover-card"]' : '[data-testid="login-form"]',
			{ visible: true, timeout: 15000 }
		);
		if (authenticated)
			await page.waitForFunction(
				() => {
					const cover = document.querySelector('[data-testid="cover-card"] img');
					return cover instanceof HTMLImageElement && cover.complete && cover.naturalWidth > 0;
				},
				{ timeout: 15000 }
			);
		const content = await page.evaluate(
			async ({ signedIn, email }) => {
				const response = await fetch('/api/v1/auth/me', { credentials: 'same-origin' });
				if (!signedIn)
					return {
						valid: response.status === 401 && !!document.querySelector('[data-testid="login-form"]')
					};
				if (!response.ok || (await response.json()).email !== email) return { valid: false };
				const collection = await fetch('/api/v1/me/collection?per_page=1', {
					credentials: 'same-origin'
				});
				const body = await collection.json();
				return { valid: collection.ok && body.total === 200, total: body.total };
			},
			{
				signedIn: authenticated,
				email: process.env.LIGHTHOUSE_AUTH_EMAIL || 'e2e-performance@lilly.app'
			}
		);
		if (!content.valid || errors.length)
			throw new Error(`Invalid page content or browser errors (${errors.length})`);
		return {
			...metrics,
			...reports,
			total: content.total ?? 0
		};
	} finally {
		await browser.close();
	}
}

async function measureInChild(origin, route, prefix) {
	const puppeteer = await import('puppeteer-core');
	const { default: lighthouse } = await import('lighthouse');
	let browserVersion;
	const result = await auditOnce(origin, route === 'collection', {
		audit: lighthouse,
		request: fetch,
		launch: async () => {
			const browser = await puppeteer.default.launch({
				executablePath: config.ci.collect.chromePath,
				...config.ci.collect.puppeteerLaunchOptions
			});
			browserVersion = await browser.version();
			return browser;
		},
		saveReport: async ({ json, html }) => {
			await fs.writeFile(`${prefix}.json`, json);
			await fs.writeFile(`${prefix}.html`, html);
		}
	});
	process.stdout.write(
		JSON.stringify({
			fcp: result.fcp,
			performance: result.performance,
			total: result.total,
			browserVersion
		})
	);
}

async function main() {
	const origin = new URL(process.env.LIGHTHOUSE_BASE_URL || 'http://localhost').origin;
	const output = path.resolve('.lighthouseci');
	await fs.mkdir(output, { recursive: true });
	// A failed new series must never leave a previous success summary behind.
	for (const name of await fs.readdir(output)) {
		if (/^(?:public|collection)-[1-5]\.(?:json|html)$|^(?:summary|failure)\.json$/.test(name))
			await fs.unlink(path.join(output, name));
	}
	const runs = [];
	const metadata = {
		commit: execFileSync('git', ['rev-parse', 'HEAD'], { encoding: 'utf8' }).trim(),
		workingTreeDirty: !!execFileSync('git', ['status', '--porcelain', '--untracked-files=no'], {
			encoding: 'utf8'
		}).trim(),
		tools: {
			node: process.versions.node,
			lighthouse: require('lighthouse/package.json').version,
			puppeteer: require('puppeteer-core/package.json').version,
			playwright: require('@playwright/test/package.json').version
		},
		recordedAt: new Date().toISOString(),
		profile: config.ci.collect.settings,
		coldBrowserPerRun: true,
		isolatedLighthouseProcessPerRun: true,
		browser: {
			variant: process.env.CHROME_PATH ? 'custom' : 'chromium-headless-shell',
			executable: path.basename(config.ci.collect.chromePath)
		},
		fixture:
			'200 entries across Maddrax and John Sinclair, 200 distinct local JPEG covers (320x480), all five conditions and three statuses; no remote cover requests'
	};
	try {
		for (const authenticated of [false, true]) {
			const route = authenticated ? 'collection' : 'public';
			for (let iteration = 1; iteration <= 5; iteration++) {
				// Keep Lighthouse's own trace processor state isolated as well as browser storage.
				const result = JSON.parse(
					execFileSync(
						process.execPath,
						[__filename, '--measure', origin, route, path.join(output, `${route}-${iteration}`)],
						{ encoding: 'utf8', windowsHide: true }
					)
				);
				metadata.tools.chromium = result.browserVersion;
				runs.push({
					route,
					iteration,
					fcp: result.fcp,
					performance: result.performance,
					total: result.total
				});
				console.log(
					`${route} ${iteration}/5: FCP ${Math.round(result.fcp)} ms; performance ${Math.round(result.performance * 100)}`
				);
			}
		}
		const summary = { ...metadata, runs, ...summarize(runs) };
		await fs.writeFile(path.join(output, 'summary.json'), JSON.stringify(summary, null, 2));
		if (!summary.passed) throw new Error('Mobile Lighthouse thresholds failed');
	} catch (error) {
		await fs.writeFile(
			path.join(output, 'failure.json'),
			JSON.stringify({ ...metadata, runs, error: error.message }, null, 2)
		);
		throw error;
	}
}

module.exports = { median, summarize, validateReport, auditOnce };
if (require.main === module)
	(process.argv[2] === '--measure'
		? measureInChild(process.argv[3], process.argv[4], process.argv[5])
		: main()
	).catch((error) => {
		console.error(error.message);
		process.exitCode = 1;
	});
