import { test, expect } from '@playwright/test';

test.describe('Register Page', () => {
	test.beforeEach(async ({ page }) => {
		await page.goto('/register');
	});

	test('displays the registration page correctly', async ({ page }) => {
		await expect(page.getByRole('heading', { name: /LILLY/i })).toBeVisible();
		await expect(page.getByLabel(/anzeigename/i)).toBeVisible();
		await expect(page.getByLabel(/e-mail/i)).toBeVisible();
		await expect(page.getByLabel(/^passwort$/i)).toBeVisible();
		await expect(page.getByLabel(/passwort bestätigen/i)).toBeVisible();
		await expect(page.getByTestId('privacy-consent-checkbox')).toBeVisible();
		await expect(page.getByRole('button', { name: /registrieren/i })).toBeVisible();
	});

	test('has a descriptive page title', async ({ page }) => {
		await expect(page).toHaveTitle(/Registrieren.*LILLY/);
	});

	test('shows validation errors for empty fields', async ({ page }) => {
		await page.getByLabel(/anzeigename/i).click();
		await page.getByLabel(/e-mail/i).click();
		await page.getByLabel(/^passwort$/i).click();
		await page.getByLabel(/passwort bestätigen/i).click();
		await page.getByRole('button', { name: /registrieren/i }).click();

		await expect(page.getByText(/Anzeigename ist erforderlich/i)).toBeVisible();
		await expect(page.getByText(/E-Mail-Adresse ist erforderlich/i)).toBeVisible();
		await expect(page.getByText(/Passwort ist erforderlich/i)).toBeVisible();
		await expect(page.getByText(/Passwortbestätigung ist erforderlich/i)).toBeVisible();
	});

	test('shows password strength indicator', async ({ page }) => {
		await page.getByLabel(/^passwort$/i).fill('testpassword123');
		await expect(page.getByTestId('password-strength')).toBeVisible();
	});

	test('shows password mismatch error', async ({ page }) => {
		await page.getByLabel(/^passwort$/i).fill('Password123!');
		const confirmInput = page.getByLabel(/passwort bestätigen/i);
		await confirmInput.fill('DifferentPassword');
		await confirmInput.blur();

		await expect(page.getByText(/Passwörter stimmen nicht überein/i)).toBeVisible();
	});

	test('OAuth buttons are disabled', async ({ page }) => {
		await expect(page.getByRole('button', { name: /google/i })).toBeDisabled();
		await expect(page.getByRole('button', { name: /github/i })).toBeDisabled();
	});

	test('has link to login page', async ({ page }) => {
		const loginLink = page.getByRole('link', { name: /anmelden/i });
		await expect(loginLink).toBeVisible();
		await expect(loginLink).toHaveAttribute('href', '/login');
	});

	test('has link to privacy policy', async ({ page }) => {
		const privacyLink = page.getByRole('link', { name: /datenschutzerklärung/i });
		await expect(privacyLink).toBeVisible();
		await expect(privacyLink).toHaveAttribute('href', '/privacy');
	});

	test('all form inputs have associated labels', async ({ page }) => {
		await expect(page.getByLabel(/anzeigename/i)).toHaveAttribute('id', 'display-name');
		await expect(page.getByLabel(/e-mail/i)).toHaveAttribute('id', 'email');
		await expect(page.getByLabel(/^passwort$/i)).toHaveAttribute('id', 'password');
		await expect(page.getByLabel(/passwort bestätigen/i)).toHaveAttribute(
			'id',
			'password-confirmation'
		);
	});

	test('form inputs have correct autocomplete attributes', async ({ page }) => {
		await expect(page.getByLabel(/anzeigename/i)).toHaveAttribute('autocomplete', 'name');
		await expect(page.getByLabel(/e-mail/i)).toHaveAttribute('autocomplete', 'email');
		await expect(page.getByLabel(/^passwort$/i)).toHaveAttribute('autocomplete', 'new-password');
		await expect(page.getByLabel(/passwort bestätigen/i)).toHaveAttribute(
			'autocomplete',
			'new-password'
		);
	});
});
