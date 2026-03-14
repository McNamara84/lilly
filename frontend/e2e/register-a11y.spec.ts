import { test, expect } from '@playwright/test';
import AxeBuilder from '@axe-core/playwright';

test.describe('Register Page Accessibility', () => {
	test.beforeEach(async ({ page }) => {
		await page.goto('/register');
	});

	test('has no automatically detectable accessibility violations', async ({ page }) => {
		const results = await new AxeBuilder({ page }).analyze();

		expect(results.violations).toEqual([]);
	});

	test('has a proper heading hierarchy', async ({ page }) => {
		const h1 = page.getByRole('heading', { level: 1 });
		await expect(h1).toBeVisible();
		await expect(h1).toHaveText('LILLY');
	});

	test('validation errors use aria-invalid and aria-describedby', async ({ page }) => {
		const emailInput = page.getByLabel(/e-mail/i);

		await emailInput.focus();
		await emailInput.blur();

		await expect(emailInput).toHaveAttribute('aria-invalid', 'true');
		await expect(emailInput).toHaveAttribute('aria-describedby', 'email-error');

		const errorText = page.locator('#email-error');
		await expect(errorText).toBeVisible();
		await expect(errorText).toHaveText(/E-Mail-Adresse ist erforderlich/i);
	});

	test('password strength indicator has correct ARIA attributes', async ({ page }) => {
		await page.getByLabel(/^passwort$/i).fill('testpassword');

		const progressbar = page.getByRole('progressbar');
		await expect(progressbar).toBeVisible();
		await expect(progressbar).toHaveAttribute('aria-valuemin', '0');
		await expect(progressbar).toHaveAttribute('aria-valuemax', '4');
	});
});
