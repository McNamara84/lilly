import AxeBuilder from '@axe-core/playwright';
import { expect, test } from '@playwright/test';

test('ARIA label references survive an axe scan and subsequent Playwright lookups', async ({
	page
}) => {
	// Firefox build 1572 crashes when both script worlds read these references.
	// https://github.com/microsoft/playwright/issues/43967
	await page.setContent(`
		<!doctype html>
		<html lang="en">
			<head><title>ARIA reference regression</title></head>
			<body>
				<main>
					<h1>Profile</h1>
					<section aria-labelledby="identity-heading">
						<h2 id="identity-heading">Identity</h2>
						<label for="display-name">Display name</label>
						<input id="display-name" type="text">
					</section>
				</main>
			</body>
		</html>
	`);

	expect((await new AxeBuilder({ page }).analyze()).violations).toEqual([]);
	await expect(page.getByRole('region', { name: 'Identity' })).toBeVisible();
	await page.getByLabel('Display name').fill('Collector');
	await expect(page.getByLabel('Display name')).toHaveValue('Collector');
});
