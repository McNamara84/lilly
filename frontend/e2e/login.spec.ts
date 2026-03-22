import { test, expect } from '@playwright/test';

test.describe('Login Page', () => {
	test.beforeEach(async ({ page }) => {
		await page.goto('/login');
	});

	test('displays the login page correctly', async ({ page }) => {
		await expect(page.getByRole('heading', { name: /LILLY/i })).toBeVisible();
		await expect(page.getByTestId('email-input')).toBeVisible();
		await expect(page.getByTestId('password-input')).toBeVisible();
		await expect(page.getByTestId('submit-button')).toBeVisible();
	});

	test('shows validation errors for empty fields', async ({ page }) => {
		await page.getByTestId('email-input').click();
		await page.getByTestId('password-input').click();
		await page.getByTestId('submit-button').click();

		await expect(page.getByText(/E-Mail-Adresse ist erforderlich/i)).toBeVisible();
		await expect(page.getByText(/Passwort ist erforderlich/i)).toBeVisible();
	});

	test('login with demo credentials succeeds', async ({ page }) => {
		await page.getByTestId('email-input').fill('demo@lilly.app');
		await page.getByTestId('password-input').fill('demo1234');
		await page.getByTestId('submit-button').click();

		// Successful login should redirect to the dashboard
		await expect(page).toHaveURL('/', { timeout: 15000 });
	});

	test('login with wrong credentials shows error', async ({ page }) => {
		await page.getByTestId('email-input').fill('wrong@email.com');
		await page.getByTestId('password-input').fill('wrongpassword');
		await page.getByTestId('submit-button').click();

		await expect(page.getByTestId('login-error')).toBeVisible();
	});

	test('OAuth buttons are disabled', async ({ page }) => {
		await expect(page.getByTestId('oauth-google')).toBeDisabled();
		await expect(page.getByTestId('oauth-github')).toBeDisabled();
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
