import { randomUUID } from 'node:crypto';
import { expect, test, type APIRequestContext, type Playwright } from '@playwright/test';

type MailpitAddress = { Address: string };
type MailpitMessageSummary = {
	ID: string;
	Subject: string;
	To: MailpitAddress[];
};
type MailpitMessages = { messages: MailpitMessageSummary[] };
type MailpitMessage = MailpitMessageSummary & { HTML: string };

async function mailpitContext(playwright: Playwright): Promise<APIRequestContext> {
	return playwright.request.newContext({
		baseURL: process.env.MAILPIT_API_URL ?? 'http://127.0.0.1:8025'
	});
}

async function waitForMail(
	mailpit: APIRequestContext,
	recipient: string,
	subject: string
): Promise<MailpitMessage> {
	let messageId: string | undefined;
	await expect
		.poll(
			async () => {
				const response = await mailpit.get('/api/v1/messages?limit=200');
				if (!response.ok()) return undefined;
				const mailbox = (await response.json()) as MailpitMessages;
				messageId = mailbox.messages.find(
					(message) =>
						message.Subject === subject &&
						message.To.some((address) => address.Address === recipient)
				)?.ID;
				return messageId;
			},
			{ timeout: 15_000, intervals: [100, 250, 500, 1_000] }
		)
		.toBeTruthy();

	const response = await mailpit.get(`/api/v1/message/${messageId}`);
	expect(response.ok()).toBe(true);
	return (await response.json()) as MailpitMessage;
}

function linkFromMail(message: MailpitMessage, route: string): string {
	const hrefs = [...message.HTML.matchAll(/href="([^"]+)"/g)].map((match) =>
		match[1].replaceAll('&amp;', '&')
	);
	const link = hrefs.find((href) => new URL(href).pathname === route);
	expect(link).toBeTruthy();
	return link!;
}

test('password reset email changes the password exactly once', async ({
	page,
	request,
	playwright
}) => {
	const id = randomUUID();
	const email = `password-reset-${id}@example.test`;
	const oldPassword = `Amber-Comet-47-Library!-${id}`;
	const newPassword = `Harbor-Violet-83-Archive!-${id}`;
	const mailpit = await mailpitContext(playwright);

	try {
		const optionsResponse = await request.get('/api/v1/auth/options');
		expect(optionsResponse.ok()).toBe(true);
		const options = (await optionsResponse.json()) as { privacy_policy: { version: string } };
		const registration = await request.post('/api/v1/auth/register', {
			data: {
				display_name: 'Reset E2E Collector',
				email,
				password: oldPassword,
				password_confirmation: oldPassword,
				privacy_consent: true,
				privacy_policy_version: options.privacy_policy.version
			}
		});
		expect(registration.status()).toBe(201);

		const verificationMail = await waitForMail(
			mailpit,
			email,
			'Bestätige deine E-Mail-Adresse – LILLY'
		);
		const verificationUrl = new URL(linkFromMail(verificationMail, '/api/v1/auth/verify'));
		const verification = await request.get(`${verificationUrl.pathname}${verificationUrl.search}`, {
			maxRedirects: 0
		});
		expect(verification.status()).toBe(303);

		await page.goto('/forgot-password');
		await page.getByLabel('E-Mail').fill(email);
		await page.getByRole('button', { name: 'Reset-Link anfordern' }).click();
		await expect(page.getByTestId('reset-request-success')).toBeVisible();

		const resetMail = await waitForMail(mailpit, email, 'Setze dein LILLY-Passwort zurück');
		const resetUrl = linkFromMail(resetMail, '/reset-password');
		await page.goto(resetUrl);
		await page.getByTestId('new-password-input').fill(newPassword);
		await page.getByTestId('new-password-confirmation-input').fill(newPassword);
		await page.getByRole('button', { name: 'Passwort ändern' }).click();
		await expect(page).toHaveURL(/\/login\?reset=true$/);
		await expect(page.getByTestId('login-success')).toBeVisible();

		const oldLogin = await request.post('/api/v1/auth/login', {
			data: { email, password: oldPassword }
		});
		expect(oldLogin.status()).toBe(401);
		const newLogin = await request.post('/api/v1/auth/login', {
			data: { email, password: newPassword }
		});
		expect(newLogin.ok()).toBe(true);

		await page.goto(resetUrl);
		await page.getByTestId('new-password-input').fill(`${newPassword}-second-use`);
		await page.getByTestId('new-password-confirmation-input').fill(`${newPassword}-second-use`);
		await page.getByRole('button', { name: 'Passwort ändern' }).click();
		await expect(page.getByTestId('reset-confirm-error')).toContainText(
			'ungültig, abgelaufen oder wurde bereits verwendet'
		);
	} finally {
		await mailpit.dispose();
	}
});
