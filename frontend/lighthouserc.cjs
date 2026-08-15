// Lighthouse CI loads its configuration through CommonJS.
// eslint-disable-next-line @typescript-eslint/no-require-imports
const { chromium } = require('@playwright/test');

const baseUrl = process.env.LIGHTHOUSE_BASE_URL || 'http://localhost';

module.exports = {
	ci: {
		collect: {
			url: [`${baseUrl}/privacy`, `${baseUrl}/collection`],
			numberOfRuns: 5,
			chromePath: process.env.CHROME_PATH || chromium.executablePath(),
			puppeteerScript: './scripts/lighthouse-auth.cjs',
			puppeteerLaunchOptions: {
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
