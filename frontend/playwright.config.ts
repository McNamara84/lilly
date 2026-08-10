import { defineConfig, devices } from '@playwright/test';

function configuredWorkers(): number {
	const requestedWorkers = process.env.PLAYWRIGHT_WORKERS;
	if (!requestedWorkers) return process.env.CI ? 2 : 3;

	const workers = Number.parseInt(requestedWorkers, 10);
	if (!Number.isInteger(workers) || workers < 1 || workers > 4) {
		throw new RangeError('PLAYWRIGHT_WORKERS must be an integer between 1 and 4');
	}
	return workers;
}

export default defineConfig({
	testDir: './e2e',
	fullyParallel: true,
	forbidOnly: !!process.env.CI,
	retries: process.env.CI ? 2 : 0,
	workers: configuredWorkers(),
	reporter: process.env.CI ? [['dot'], ['html', { open: 'never' }]] : [['html', { open: 'never' }]],
	use: {
		baseURL: 'http://localhost:80',
		trace: 'on-first-retry',
		screenshot: 'only-on-failure'
	},
	projects: [
		{
			name: 'chromium',
			use: { ...devices['Desktop Chrome'] }
		},
		{
			name: 'firefox',
			use: { ...devices['Desktop Firefox'] }
		},
		{
			name: 'webkit',
			use: { ...devices['Desktop Safari'] }
		},
		{
			name: 'mobile-chrome',
			use: { ...devices['Pixel 5'] }
		}
	]
});
