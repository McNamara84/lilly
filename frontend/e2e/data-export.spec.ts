import { execFileSync } from 'node:child_process';
import AxeBuilder from '@axe-core/playwright';
import { test, expect } from './fixtures';

const PNG = Buffer.from(
	'iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAIAAAD91JpzAAAAFklEQVQI12P0WefPwMDAxMDAwMDAAAAPkwFN/OEGTAAAAABJRU5ErkJggg==',
	'base64'
);
const inspectArchive = String.raw`
import hashlib, json, sys, zipfile
with zipfile.ZipFile(sys.argv[1]) as archive:
    assert archive.testzip() is None
    manifest = json.loads(archive.read('manifest.json'))
    for name, record in manifest['files'].items():
        content = archive.read(name)
        assert len(content) == record['bytes']
        assert hashlib.sha256(content).hexdigest() == record['sha256']
    print(json.dumps({'files': archive.namelist(), 'account': json.loads(archive.read('data/account.json')), 'collection': json.loads(archive.read('collection.json')), 'photos': json.loads(archive.read('data/photos.json')), 'manifest': manifest}))
`;

test('personal ZIP contains the actual own data and files and denies other accounts', async ({
	page,
	anonymousRequest
}, testInfo) => {
	test.setTimeout(90_000);
	const collection = (await (
		await page.request.get('/api/v1/me/collection?per_page=1')
	).json()) as { data: Array<{ issue_id: number }> };
	const own = (await (await page.request.get('/api/v1/auth/me')).json()) as {
		id: number;
		email: string;
	};
	const entryResponse = await page.request.post('/api/v1/me/collection', {
		data: {
			issue_id: collection.data[0].issue_id,
			copy_number: 210,
			status: 'owned',
			condition_grade: 'Z2',
			edition_label: 'Export edition',
			notes: 'Personal export proof'
		}
	});
	expect(entryResponse.status()).toBe(201);
	const entry = (await entryResponse.json()) as { id: number };
	try {
		const photo = await page.request.post(`/api/v1/me/collection/${entry.id}/photos`, {
			multipart: { photo: { name: 'own.png', mimeType: 'image/png', buffer: PNG } }
		});
		expect(photo.status()).toBe(201);
		await page.goto('/profile');
		await page.getByTestId('personal-data-export-link').click();
		await expect(page.getByTestId('request-personal-export')).toBeEnabled();
		const requested = page.waitForResponse(
			(response) =>
				response.url().endsWith('/api/v1/me/data-exports') && response.request().method() === 'POST'
		);
		await page.getByTestId('request-personal-export').click();
		const creation = await requested;
		expect(creation.status()).toBe(202);
		const job = (await creation.json()) as { id: string };
		const jobUrl = `/api/v1/me/data-exports/${job.id}`;
		await expect
			.poll(
				async () => {
					const status = await page.request.get(jobUrl);
					expect(status.ok()).toBe(true);
					expect(status.headers()['cache-control']).toBe('no-store');
					return (await status.json()).status;
				},
				{ timeout: 30_000 }
			)
			.toBe('ready');
		const row = page.getByTestId('personal-export-job').first();
		await expect(row.getByTestId('download-personal-export')).toBeVisible({ timeout: 30_000 });
		await expect(row).toContainText('Download verfügbar bis');
		const jobBefore = (await (await page.request.get(jobUrl)).json()) as {
			expires_at: string;
			completed_at: string;
		};
		expect(Date.parse(jobBefore.expires_at) - Date.parse(jobBefore.completed_at)).toBe(86_400_000);
		expect(
			(await anonymousRequest.get(`/api/v1/me/data-exports/${job.id}/download`)).status()
		).toBe(401);
		const adminLogin = await anonymousRequest.post('/api/v1/auth/login', {
			data: { email: 'demo@lilly.app', password: 'demo1234' }
		});
		expect(adminLogin.ok()).toBe(true);
		expect((await anonymousRequest.get(`/api/v1/me/data-exports/${job.id}`)).status()).toBe(404);
		expect(
			(await anonymousRequest.get(`/api/v1/me/data-exports/${job.id}/download`)).status()
		).toBe(404);
		const downloaded = page.waitForEvent('download');
		await row.getByTestId('download-personal-export').click();
		const archivePath = testInfo.outputPath('personal-data.zip');
		const archive = await downloaded;
		expect(archive.suggestedFilename()).toBe(`lilly-personal-data-${job.id}.zip`);
		await archive.saveAs(archivePath);
		const inspected = JSON.parse(
			execFileSync(
				process.platform === 'win32' ? 'python' : 'python3',
				['-c', inspectArchive, archivePath],
				{ encoding: 'utf8', windowsHide: true }
			)
		) as {
			files: string[];
			account: Array<{ email: string }>;
			collection: { entries: Array<{ notes: string }> };
			photos: Array<{ entry_id: number }>;
			manifest: { format: string };
		};
		expect(inspected.account[0].email).toBe(own.email);
		expect(
			inspected.collection.entries.some((item) => item.notes === 'Personal export proof')
		).toBe(true);
		expect(inspected.photos.some((item) => item.entry_id === entry.id)).toBe(true);
		expect(inspected.files.some((name) => name.startsWith('files/photos/'))).toBe(true);
		expect(inspected.manifest.format).toBe('lilly.personal-data');
		expect(JSON.stringify(inspected.account)).not.toMatch(/password_hash|erasure_subject|token/);
		const after = (await (await page.request.get(jobUrl)).json()) as {
			expires_at: string;
		};
		expect(after.expires_at).toBe(jobBefore.expires_at);
		const accessibility = await new AxeBuilder({ page }).analyze();
		expect(accessibility.violations).toEqual([]);
	} finally {
		await page.request.delete(`/api/v1/me/collection/${entry.id}`);
	}
});

test.describe('Personal export reauthentication', () => {
	test.use({ serviceWorkers: 'block' });
	test('offers password confirmation before generating an export', async ({ page }) => {
		let first = true;
		await page.route('**/api/v1/me/reauthentication/options', async (route) => {
			const response = await route.fetch();
			const body = await response.json();
			if (first) {
				body.recent_authentication = false;
				first = false;
			}
			await route.fulfill({ response, json: body });
		});
		await page.goto('/account/data-export');
		await expect(page.getByTestId('export-reauth')).toBeVisible();
		await expect(page.getByTestId('request-personal-export')).toBeDisabled();
		await page.getByLabel('Aktuelles Passwort').fill('e2e-worker-password');
		await page.getByTestId('export-confirm-password').click();
		await expect(page.getByText('Deine Anmeldung ist bestätigt.')).toBeVisible();
		await expect(page.getByTestId('request-personal-export')).toBeEnabled();
	});
});
