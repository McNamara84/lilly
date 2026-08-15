import { describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
	getCachedProfile: vi.fn().mockResolvedValue({ id: 1 }),
	mergeCollectionEntries: vi.fn().mockRejectedValue(new Error('IndexedDB unavailable'))
}));

vi.mock('$lib/offline/database', () => ({
	getCachedProfile: mocks.getCachedProfile,
	listStoredCollectionEntries: vi.fn(),
	mergeCollectionEntries: mocks.mergeCollectionEntries,
	replaceOfflineSnapshot: vi.fn()
}));

import { fetchCollection } from '$lib/api/collection';

describe('collection API cache failures', () => {
	it('does not fail a successful request when background cache merging fails', async () => {
		const data = {
			data: [{ id: 1 }],
			page: 1,
			per_page: 50,
			total: 1
		};
		vi.stubGlobal(
			'fetch',
			vi.fn().mockResolvedValue({ ok: true, status: 200, json: async () => data })
		);

		await expect(fetchCollection()).resolves.toEqual(data);
		await vi.waitFor(() => expect(mocks.mergeCollectionEntries).toHaveBeenCalledOnce());
	});
});
