// Lighthouse CI loads its configuration through CommonJS.
// @ts-expect-error Playwright exports its browser registry without a TypeScript declaration.
const { registry } = require('playwright-core/lib/coreBundle'); // eslint-disable-line @typescript-eslint/no-require-imports

const baseUrl = process.env.LIGHTHOUSE_BASE_URL || 'http://localhost';

module.exports = {
	ci: {
		collect: {
			url: [`${baseUrl}/`, `${baseUrl}/collection`],
			numberOfRuns: 5,
			// Use the same installed headless engine as Playwright; resolve its platform-specific path.
			chromePath:
				process.env.CHROME_PATH ||
				registry.registry.findExecutable('chromium-headless-shell').executablePath(),
			puppeteerLaunchOptions: {
				headless: process.env.CHROME_PATH ? true : 'shell',
				args: ['--no-sandbox', '--disable-dev-shm-usage']
			},
			settings: {
				formFactor: 'mobile',
				throttlingMethod: 'simulate',
				throttling: {
					rttMs: 150,
					throughputKbps: 1638.4,
					requestLatencyMs: 562.5,
					downloadThroughputKbps: 1474.56,
					uploadThroughputKbps: 675,
					cpuSlowdownMultiplier: 4
				},
				screenEmulation: {
					mobile: true,
					width: 412,
					height: 823,
					deviceScaleFactor: 1.75,
					disabled: false
				},
				disableStorageReset: false
			}
		},
		assert: {
			aggregationMethod: 'median',
			assertions: {
				'categories:performance': ['error', { minScore: 0.9 }],
				'first-contentful-paint': ['error', { maxNumericValue: 3000 }]
			}
		},
		upload: {
			target: 'filesystem',
			outputDir: './.lighthouseci'
		}
	}
};
