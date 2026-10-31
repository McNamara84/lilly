import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

describe('offline session tombstones', () => {
	beforeEach(() => {
		vi.resetModules();
		localStorage.clear();
	});
	afterEach(() => vi.restoreAllMocks());

	it('invalidates old responses and keeps logout durable across module reloads', async () => {
		let session = await import('#lib/offline/session.js');
		session.changeOfflineSession(1, false);
		const active = session.offlineSessionVersion();
		session.changeOfflineSession(1, true);
		expect(session.isCurrentOfflineSession(active)).toBe(false);
		vi.resetModules();
		session = await import('#lib/offline/session.js');
		expect(session.isOfflineUserBlocked(1)).toBe(true);
		expect(session.isOfflineUserBlocked(2)).toBe(false);
		session.changeOfflineSession(1, false);
		expect(session.isOfflineUserBlocked(1)).toBe(false);
		expect(session.isCurrentOfflineSession(active)).toBe(false);
	});

	it('still deactivates the current page if browser storage rejects writes', async () => {
		const session = await import('#lib/offline/session.js');
		session.changeOfflineSession(1, false);
		const version = session.offlineSessionVersion();
		vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
			throw new DOMException('Full', 'QuotaExceededError');
		});
		expect(() => session.changeOfflineSession(1, true)).not.toThrow();
		expect(session.isCurrentOfflineSession(version)).toBe(false);
		expect(session.isOfflineUserBlocked(1)).toBe(true);
		session.changeOfflineSession(2, false);
		expect(session.isOfflineUserBlocked(2)).toBe(false);
	});
});
