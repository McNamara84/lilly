import { beforeEach, describe, expect, it, vi } from 'vitest';
import {
	fetchOwnProfile,
	fetchPublicCollection,
	fetchPublicCollectionStats,
	fetchPublicProfile,
	updateVisibility
} from '$lib/api/profile';

const mockFetch = vi.fn();
globalThis.fetch = mockFetch;

function jsonResponse(data: unknown, status = 200) {
	return {
		ok: status >= 200 && status < 300,
		status,
		json: () => Promise.resolve(data)
	};
}

describe('profile API', () => {
	beforeEach(() => vi.clearAllMocks());

	it('fetches the authenticated profile with both visibility flags', async () => {
		const profile = {
			id: 7,
			email: 'sammler@example.com',
			display_name: 'Sammler',
			avatar_path: null,
			location: 'Berlin',
			profile_public: false,
			collection_public: true,
			created_at: '2026-01-01T00:00:00'
		};
		mockFetch.mockResolvedValue(jsonResponse(profile));

		await expect(fetchOwnProfile()).resolves.toEqual(profile);
		expect(mockFetch).toHaveBeenCalledWith('/api/v1/me/profile', {
			credentials: 'same-origin'
		});
	});

	it('updates profile and collection visibility independently', async () => {
		const settings = { profile_public: false, collection_public: true };
		mockFetch.mockResolvedValue(jsonResponse(settings));

		await expect(updateVisibility(settings)).resolves.toEqual(settings);
		expect(mockFetch).toHaveBeenCalledWith('/api/v1/me/profile/visibility', {
			method: 'PATCH',
			headers: { 'Content-Type': 'application/json' },
			credentials: 'same-origin',
			body: JSON.stringify(settings)
		});
	});

	it('fetches a public profile without authenticated request options', async () => {
		const profile = {
			id: 7,
			display_name: 'Sammler',
			avatar_path: null,
			location: null,
			created_at: '2026-01-01T00:00:00'
		};
		mockFetch.mockResolvedValue(jsonResponse(profile));

		await expect(fetchPublicProfile(7)).resolves.toEqual(profile);
		expect(mockFetch).toHaveBeenCalledWith('/api/v1/users/7/profile');
	});

	it('fetches a paginated public collection', async () => {
		const collection = { data: [], page: 2, per_page: 25, total: 0 };
		mockFetch.mockResolvedValue(jsonResponse(collection));

		await expect(fetchPublicCollection(7, 2, 25)).resolves.toEqual(collection);
		expect(mockFetch).toHaveBeenCalledWith('/api/v1/users/7/collection?page=2&per_page=25');
	});

	it('uses stable defaults for public collection pagination', async () => {
		mockFetch.mockResolvedValue(jsonResponse({ data: [], page: 1, per_page: 100, total: 0 }));

		await fetchPublicCollection(7);

		expect(mockFetch).toHaveBeenCalledWith('/api/v1/users/7/collection?page=1&per_page=100');
	});

	it('fetches public collection stats', async () => {
		const stats = {
			total_issues: 10,
			total_owned: 2,
			total_duplicate: 1,
			total_wanted: 3,
			overall_progress_percent: 20,
			series_stats: []
		};
		mockFetch.mockResolvedValue(jsonResponse(stats));

		await expect(fetchPublicCollectionStats(7)).resolves.toEqual(stats);
		expect(mockFetch).toHaveBeenCalledWith('/api/v1/users/7/collection/stats');
	});

	it('preserves the status code when a public resource is private or absent', async () => {
		mockFetch.mockResolvedValue(jsonResponse({ error: 'Resource not found' }, 404));

		const error = await fetchPublicProfile(7).catch((cause: unknown) => cause);

		expect(error).toBeInstanceOf(Error);
		expect(error).toMatchObject({ message: 'Resource not found', status: 404 });
	});

	it('uses a generic message if an error response has no usable JSON body', async () => {
		mockFetch.mockResolvedValue({
			ok: false,
			status: 500,
			json: () => Promise.reject(new Error('invalid json'))
		});

		await expect(fetchOwnProfile()).rejects.toMatchObject({
			message: 'An unexpected error occurred',
			status: 500
		});
	});
});
