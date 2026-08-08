import { test, expect, type Page } from '@playwright/test';
import AxeBuilder from '@axe-core/playwright';

interface OwnProfile {
	id: number;
	email: string;
	display_name: string;
	profile_public: boolean;
	collection_public: boolean;
}

interface CollectionEntry {
	id: number;
	issue_id: number;
	copy_number: number | null;
	condition_grade: string | null;
	status: 'owned' | 'duplicate' | 'wanted';
	notes: string | null;
}

async function loginAsDemo(page: Page) {
	await page.goto('/login');
	await page.getByTestId('email-input').fill('demo@lilly.app');
	await page.getByTestId('password-input').fill('demo1234');
	await page.getByTestId('submit-button').click();
	await expect(page).toHaveURL('/', { timeout: 15000 });
}

async function ownProfile(page: Page): Promise<OwnProfile> {
	const response = await page.request.get('/api/v1/me/profile');
	expect(response.ok()).toBe(true);
	return response.json();
}

async function setVisibility(page: Page, profilePublic: boolean, collectionPublic: boolean) {
	const response = await page.request.patch('/api/v1/me/profile/visibility', {
		data: {
			profile_public: profilePublic,
			collection_public: collectionPublic
		}
	});
	expect(response.ok()).toBe(true);
}

test.describe('Profile Visibility and Public Notes', () => {
	test.beforeEach(async ({ page }) => {
		await loginAsDemo(page);
	});

	test('profile settings expose independent toggles and the note warning', async ({
		page,
		request
	}) => {
		const snapshot = await ownProfile(page);
		try {
			await setVisibility(page, false, false);
			await page.goto('/profile');

			await expect(page.getByTestId('profile-public-toggle')).not.toBeChecked();
			await expect(page.getByTestId('collection-public-toggle')).not.toBeChecked();
			await expect(
				page.getByText('Öffentliche Sammlungen zeigen auch deine persönlichen Heftnotizen.')
			).toBeVisible();

			await page.getByTestId('collection-public-toggle').check();
			await page.getByTestId('save-visibility').click();
			await expect(page.getByTestId('profile-success')).toHaveText('Sichtbarkeit gespeichert.');

			const saved = await ownProfile(page);
			expect(saved.profile_public).toBe(false);
			expect(saved.collection_public).toBe(true);
			await expect(
				page.getByRole('link', { name: 'Öffentliche Sammlung ansehen' })
			).toHaveAttribute('href', `/users/${saved.id}/collection`);

			// The global request fixture has no browser login cookies: this verifies
			// that a direct public-collection URL really is unauthenticated.
			expect((await request.get(`/api/v1/users/${saved.id}/profile`)).status()).toBe(404);
			expect((await request.get(`/api/v1/users/${saved.id}/collection`)).status()).toBe(200);
		} finally {
			await setVisibility(page, snapshot.profile_public, snapshot.collection_public);
		}
	});

	test('all four profile and collection visibility combinations follow the API matrix', async ({
		page,
		request
	}) => {
		const snapshot = await ownProfile(page);
		const combinations = [
			{ profile: false, collection: false },
			{ profile: true, collection: false },
			{ profile: false, collection: true },
			{ profile: true, collection: true }
		];

		try {
			for (const visibility of combinations) {
				await setVisibility(page, visibility.profile, visibility.collection);

				const profileResponse = await request.get(`/api/v1/users/${snapshot.id}/profile`);
				const collectionResponse = await request.get(
					`/api/v1/users/${snapshot.id}/collection?page=1&per_page=100`
				);
				const statsResponse = await request.get(`/api/v1/users/${snapshot.id}/collection/stats`);

				expect(profileResponse.status()).toBe(visibility.profile ? 200 : 404);
				expect(collectionResponse.status()).toBe(visibility.collection ? 200 : 404);
				expect(statsResponse.status()).toBe(visibility.collection ? 200 : 404);

				if (visibility.profile) {
					const publicProfile = await profileResponse.json();
					expect(publicProfile).not.toHaveProperty('email');
					expect(publicProfile).not.toHaveProperty('role');
					expect(publicProfile).not.toHaveProperty('profile_public');
					expect(publicProfile).not.toHaveProperty('collection_public');
				}

				if (visibility.collection) {
					const publicCollection = (await collectionResponse.json()) as { data: unknown[] };
					for (const entry of publicCollection.data) {
						expect(entry).not.toHaveProperty('id');
						expect(entry).not.toHaveProperty('user_id');
						expect(entry).not.toHaveProperty('email');
						expect(entry).toHaveProperty('notes');
					}
				}
			}
		} finally {
			await setVisibility(page, snapshot.profile_public, snapshot.collection_public);
		}
	});

	test('unicode notes can be changed, published and removed by clearing them', async ({
		page,
		request
	}) => {
		const profileSnapshot = await ownProfile(page);
		const collectionResponse = await page.request.get('/api/v1/me/collection?per_page=100');
		expect(collectionResponse.ok()).toBe(true);
		const collection = (await collectionResponse.json()) as { data: CollectionEntry[] };
		expect(collection.data.length).toBeGreaterThan(0);
		const entry = collection.data[0];
		const note = 'Erste Zeile\nGrüße aus Köln 📚 <script>alert(1)</script>';

		try {
			await setVisibility(page, false, true);
			const updateResponse = await page.request.patch(`/api/v1/me/collection/${entry.id}`, {
				data: { notes: note }
			});
			expect(updateResponse.ok()).toBe(true);
			expect((await updateResponse.json()).notes).toBe(note);

			const publicResponse = await request.get(
				`/api/v1/users/${profileSnapshot.id}/collection?page=1&per_page=100`
			);
			expect(publicResponse.ok()).toBe(true);
			const publicCollection = (await publicResponse.json()) as {
				data: Array<{ issue_id: number; copy_number: number; notes: string | null }>;
			};
			const publicEntry = publicCollection.data.find(
				(item) => item.issue_id === entry.issue_id && item.copy_number === entry.copy_number
			);
			expect(publicEntry?.notes).toBe(note);

			await page.goto(`/users/${profileSnapshot.id}/collection`);
			await expect(
				page.getByTestId('collection-note').filter({ hasText: 'Grüße aus Köln' })
			).toBeVisible();
			await expect(page.locator('script')).toHaveCount(0);

			const clearResponse = await page.request.patch(`/api/v1/me/collection/${entry.id}`, {
				data: { notes: '' }
			});
			expect(clearResponse.ok()).toBe(true);
			expect((await clearResponse.json()).notes).toBeNull();
		} finally {
			const restoreResponse = await page.request.patch(`/api/v1/me/collection/${entry.id}`, {
				data: { notes: entry.notes ?? '' }
			});
			expect(restoreResponse.ok()).toBe(true);
			await setVisibility(page, profileSnapshot.profile_public, profileSnapshot.collection_public);
		}
	});

	test('profile settings and public collection have no detectable accessibility violations', async ({
		page
	}) => {
		const snapshot = await ownProfile(page);
		try {
			await setVisibility(page, snapshot.profile_public, true);
			await page.goto('/profile');
			await expect(page.getByTestId('save-visibility')).toBeVisible();
			expect((await new AxeBuilder({ page }).analyze()).violations).toEqual([]);

			await page.goto(`/users/${snapshot.id}/collection`);
			await expect(page.getByRole('heading', { name: 'Öffentliche Sammlung' })).toBeVisible();
			expect((await new AxeBuilder({ page }).analyze()).violations).toEqual([]);
		} finally {
			await setVisibility(page, snapshot.profile_public, snapshot.collection_public);
		}
	});
});
