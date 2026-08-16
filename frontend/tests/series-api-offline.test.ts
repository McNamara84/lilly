import 'fake-indexeddb/auto';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fetchAllSeriesIssues, fetchIssue, fetchSeries } from '$lib/api/series';
import {
	replaceOfflineSnapshot,
	resetOfflineDatabaseForTests,
	saveConfirmedProfile
} from '$lib/offline/database';
import { issue, profile, series, snapshot } from './fixtures/offline';

const fetchMock = vi.fn();
vi.stubGlobal('fetch', fetchMock);

describe('series API offline fallback', () => {
	beforeEach(async () => {
		fetchMock.mockReset();
		await resetOfflineDatabaseForTests();
		await saveConfirmedProfile(profile(1));
		await replaceOfflineSnapshot(snapshot(1));
		fetchMock.mockRejectedValue(new TypeError('Network unavailable'));
	});

	it('reads series, all series issues and individual issues from the snapshot', async () => {
		await expect(fetchSeries()).resolves.toEqual([series]);
		await expect(fetchAllSeriesIssues('maddrax')).resolves.toEqual([issue]);
		await expect(fetchIssue(issue.id)).resolves.toEqual(issue);
	});

	it('returns an empty list when the requested series is not cached', async () => {
		await expect(fetchAllSeriesIssues('john-sinclair')).resolves.toEqual([]);
	});

	it('rethrows the network failure when an issue is not cached', async () => {
		await expect(fetchIssue(999)).rejects.toThrow('Network unavailable');
	});

	it('does not expose offline data without a confirmed profile', async () => {
		await resetOfflineDatabaseForTests();

		await expect(fetchSeries()).rejects.toThrow('Network unavailable');
		await expect(fetchAllSeriesIssues('maddrax')).rejects.toThrow('Network unavailable');
		await expect(fetchIssue(issue.id)).rejects.toThrow('Network unavailable');
	});
});
