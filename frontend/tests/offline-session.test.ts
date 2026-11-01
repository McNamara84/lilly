import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

describe('offline session tombstones', () => {
	it('accepts the same logout epoch across storage failures and rejects superseded or malformed epochs', async () => {
		const session = await import('#lib/offline/session.js');
		session.changeOfflineSession(1, false);
		const active = session.offlineSessionVersion();
		expect(session.acceptOfflineSessionEnd(1, active, 'active:1:not-a-logout')).toBe(false);
		vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
			throw new Error('storage unavailable');
		});
		expect(session.acceptOfflineSessionEnd(1, active, 'blocked:1:remote-epoch')).toBe(true);
		expect(session.offlineSessionVersion()).toBe('blocked:1:remote-epoch');
		expect(session.isOfflineUserBlocked(1)).toBe(true);
		expect(session.acceptOfflineSessionEnd(1, active, 'blocked:1:remote-epoch')).toBe(true);
		session.changeOfflineSession(1, false);
		const current = session.offlineSessionVersion();
		expect(session.acceptOfflineSessionEnd(1, active, 'blocked:1:remote-epoch')).toBe(false);
		expect(session.offlineSessionVersion()).toBe(current);
		expect(session.isOfflineUserBlocked(1)).toBe(false);
	});
	beforeEach(() => {
		vi.resetModules();
		localStorage.clear();
	});
	afterEach(() => {
		vi.restoreAllMocks();
		vi.unstubAllGlobals();
	});

	it('uses the in-memory session and blocked users when storage reads fail', async () => {
		const session = await import('#lib/offline/session.js');
		session.changeOfflineSession(1, true);
		const version = session.offlineSessionVersion();
		vi.spyOn(Storage.prototype, 'getItem').mockImplementation(() => {
			throw new DOMException('Blocked', 'SecurityError');
		});
		expect(session.offlineSessionVersion()).toBe(version);
		expect(session.isOfflineUserBlocked(1)).toBe(true);
		expect(session.isOfflineUserBlocked(2)).toBe(false);
	});

	it('can inspect a session when localStorage is unavailable', async () => {
		const session = await import('#lib/offline/session.js');
		vi.stubGlobal('localStorage', undefined);
		expect(session.offlineSessionVersion()).toBe('');
		expect(session.isOfflineUserBlocked(1)).toBe(false);
	});

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
