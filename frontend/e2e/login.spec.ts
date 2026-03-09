import { test, expect } from '@playwright/test';

test.describe('Login Page', () => {
	test.beforeEach(async ({ page }) => {
		await page.goto('/login');
	});

	test('displays the login page correctly', async ({ page }) => {
		await expect(page.getByRole('heading', { name: /LILLY/i })).toBeVisible();
		await expect(page.getByLabel(/e-mail/i)).toBeVisible();
		await expect(page.getByLabel(/passwort/i)).toBeVisible();
		await expect(page.getByRole('button', { name: /anmelden/i })).toBeVisible();
	});

	test('shows validation errors for empty fields', async ({ page }) => {
		await page.getByLabel(/e-mail/i).click();
		await page.getByLabel(/passwort/i).click();
		await page.getByRole('button', { name: /anmelden/i }).click();

		await expect(page.getByText(/E-Mail-Adresse ist erforderlich/i)).toBeVisible();
		await expect(page.getByText(/Passwort ist erforderlich/i)).toBeVisible();
	});

	test('login with demo credentials succeeds', async ({ page }) => {
		await page.getByLabel(/e-mail/i).fill('demo@lilly.app');
		await page.getByLabel(/passwort/i).fill('demo1234');
		await page.getByRole('button', { name: /anmelden/i }).click();

		// Wait for the login request to complete (button re-enables after loading)
		await expect(page.getByRole('button', { name: /anmelden/i })).toBeEnabled();
		await expect(page.getByRole('alert')).not.toBeVisible();
	});

	test('login with wrong credentials shows error', async ({ page }) => {
		await page.getByLabel(/e-mail/i).fill('wrong@email.com');
		await page.getByLabel(/passwort/i).fill('wrongpassword');
		await page.getByRole('button', { name: /anmelden/i }).click();

		await expect(page.getByRole('alert')).toBeVisible();
	});

	test('OAuth buttons are disabled', async ({ page }) => {
		await expect(page.getByRole('button', { name: /google/i })).toBeDisabled();
		await expect(page.getByRole('button', { name: /github/i })).toBeDisabled();
	});

	test('page is accessible with correct labels', async ({ page }) => {
		const emailInput = page.getByLabel(/e-mail/i);
		const passwordInput = page.getByLabel(/passwort/i);

		await expect(emailInput).toHaveAttribute('type', 'email');
		await expect(passwordInput).toHaveAttribute('type', 'password');
		await expect(emailInput).toHaveAttribute('autocomplete', 'email');
		await expect(passwordInput).toHaveAttribute('autocomplete', 'current-password');
	});
});
