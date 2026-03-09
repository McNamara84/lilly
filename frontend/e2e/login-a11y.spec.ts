import { test, expect } from '@playwright/test';
import AxeBuilder from '@axe-core/playwright';

test.describe('Login Page Accessibility', () => {
	test.beforeEach(async ({ page }) => {
		await page.goto('/login');
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

	test('has a descriptive page title', async ({ page }) => {
		await expect(page).toHaveTitle(/Anmelden.*LILLY/);
	});

	test('all form inputs have associated labels', async ({ page }) => {
		const emailInput = page.getByLabel(/e-mail/i);
		const passwordInput = page.getByLabel(/passwort/i);

		await expect(emailInput).toBeVisible();
		await expect(passwordInput).toBeVisible();

		await expect(emailInput).toHaveAttribute('id', 'email');
		await expect(passwordInput).toHaveAttribute('id', 'password');
	});

	test('form inputs have correct autocomplete attributes', async ({ page }) => {
		await expect(page.getByLabel(/e-mail/i)).toHaveAttribute('autocomplete', 'email');
		await expect(page.getByLabel(/passwort/i)).toHaveAttribute('autocomplete', 'current-password');
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

	test('error messages are announced via role="alert"', async ({ page }) => {
		await page.getByLabel(/e-mail/i).fill('wrong@email.com');
		await page.getByLabel(/passwort/i).fill('wrongpassword');
		await page.getByRole('button', { name: /anmelden/i }).click();

		const alert = page.getByRole('alert');
		await expect(alert).toBeVisible();
	});

	test('submit button is keyboard-accessible', async ({ page }) => {
		await page.getByLabel(/e-mail/i).fill('demo@lilly.app');
		await page.getByLabel(/passwort/i).fill('demo1234');

		await page.keyboard.press('Enter');

		// Form should be submitted, no validation errors visible
		await expect(page.getByText(/E-Mail-Adresse ist erforderlich/i)).not.toBeVisible();
		await expect(page.getByText(/Passwort ist erforderlich/i)).not.toBeVisible();
	});

	test('focus order follows logical reading order', async ({ page }) => {
		await page.keyboard.press('Tab');
		const firstFocused = page.getByLabel(/e-mail/i);
		await expect(firstFocused).toBeFocused();

		await page.keyboard.press('Tab');
		const secondFocused = page.getByLabel(/passwort/i);
		await expect(secondFocused).toBeFocused();

		await page.keyboard.press('Tab');
		const thirdFocused = page.getByRole('button', { name: /anmelden/i });
		await expect(thirdFocused).toBeFocused();
	});

	test('disabled OAuth buttons are not keyboard-focusable', async ({ page }) => {
		const googleBtn = page.getByRole('button', { name: /google/i });
		const githubBtn = page.getByRole('button', { name: /github/i });

		await expect(googleBtn).toBeDisabled();
		await expect(githubBtn).toBeDisabled();
	});
});
