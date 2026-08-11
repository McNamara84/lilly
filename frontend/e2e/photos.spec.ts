import { expect, test } from './fixtures';

const PNG = Buffer.from(
	'iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAIAAAD91JpzAAAAFklEQVQI12P0WefPwMDAxMDAwMDAAAAPkwFN/OEGTAAAAABJRU5ErkJggg==',
	'base64'
);

test.describe('Personal issue photos', () => {
	test('upload, quota, persistence, deletion and collection privacy work end to end', async ({
		page,
		anonymousRequest
	}) => {
		const collectionResponse = await page.request.get('/api/v1/me/collection?per_page=100');
		expect(collectionResponse.ok()).toBe(true);
		const collection = (await collectionResponse.json()) as {
			data: Array<{ id: number; issue_id: number; issue_number: number }>;
		};
		const entry = collection.data.find((candidate) => candidate.issue_number === 1);
		expect(entry).toBeDefined();
		const entryId = entry!.id;

		const existingResponse = await page.request.get(`/api/v1/me/collection/${entryId}/photos`);
		expect(existingResponse.ok()).toBe(true);
		for (const photo of (await existingResponse.json()) as Array<{ id: number }>) {
			await page.request.delete(`/api/v1/me/collection/${entryId}/photos/${photo.id}`);
		}
		const resetVisibility = await page.request.patch('/api/v1/me/profile/visibility', {
			data: { profile_public: false, collection_public: false }
		});
		expect(resetVisibility.ok()).toBe(true);

		await page.goto(`/issues/${entry!.issue_id}`);
		await expect(page.getByTestId('photo-uploader')).toBeVisible();
		await expect(page.getByTestId('photo-count')).toHaveText('0/4');
		const input = page.getByTestId('photo-file-input');
		await expect(input).toHaveAttribute('capture', 'environment');

		for (let index = 1; index <= 4; index += 1) {
			await input.setInputFiles({
				name: `condition-${index}.png`,
				mimeType: 'image/png',
				buffer: PNG
			});
			await expect(page.getByTestId('photo-count')).toHaveText(`${index}/4`);
		}
		await expect(page.getByTestId('photo-dropzone')).toBeDisabled();
		await expect(page.getByText('Alle vier Foto-Slots sind belegt')).toBeVisible();

		const fifth = await page.request.post(`/api/v1/me/collection/${entryId}/photos`, {
			multipart: {
				photo: { name: 'fifth.png', mimeType: 'image/png', buffer: PNG }
			}
		});
		expect(fifth.status()).toBe(409);

		await page.reload();
		await expect(page.getByTestId('photo-count')).toHaveText('4/4');
		const photosResponse = await page.request.get(`/api/v1/me/collection/${entryId}/photos`);
		const photos = (await photosResponse.json()) as Array<{ id: number; content_url: string }>;
		expect(photos).toHaveLength(4);
		const ownProfile = (await (await page.request.get('/api/v1/me/profile')).json()) as {
			collection_public: boolean;
		};
		expect(ownProfile.collection_public).toBe(false);
		expect((await anonymousRequest.get('/api/v1/me/profile')).status()).toBe(401);
		expect((await anonymousRequest.get('/media/user-photos/not-public.jpg')).status()).toBe(404);
		expect((await anonymousRequest.get(photos[0].content_url)).status()).toBe(404);

		const makePublic = await page.request.patch('/api/v1/me/profile/visibility', {
			data: { profile_public: false, collection_public: true }
		});
		expect(makePublic.ok()).toBe(true);
		const publicPhoto = await anonymousRequest.get(photos[0].content_url);
		expect(publicPhoto.ok()).toBe(true);
		expect(publicPhoto.headers()['content-type']).toBe('image/jpeg');

		const makePrivate = await page.request.patch('/api/v1/me/profile/visibility', {
			data: { profile_public: false, collection_public: false }
		});
		expect(makePrivate.ok()).toBe(true);
		expect((await anonymousRequest.get(photos[0].content_url)).status()).toBe(404);

		page.once('dialog', (dialog) => dialog.accept());
		await page.getByTestId(`delete-photo-${photos[0].id}`).click();
		await expect(page.getByTestId('photo-count')).toHaveText('3/4');
		await expect(page.getByTestId('photo-dropzone')).toBeEnabled();

		const remaining = (await (
			await page.request.get(`/api/v1/me/collection/${entryId}/photos`)
		).json()) as Array<{ id: number }>;
		for (const photo of remaining) {
			const response = await page.request.delete(
				`/api/v1/me/collection/${entryId}/photos/${photo.id}`
			);
			expect(response.status()).toBe(204);
		}
	});
});
