// A synchronous tombstone also protects reloads if IndexedDB cleanup fails.
const KEY = 'lilly-offline-session';
let fallback = '';
let storageUnavailable = false;
const blockedUsers = new Set<number>();

function read(): string {
	if (storageUnavailable) return fallback;
	try {
		return typeof localStorage === 'undefined' ? fallback : (localStorage.getItem(KEY) ?? '');
	} catch {
		return fallback;
	}
}

export function offlineSessionVersion(): string {
	return read();
}

export function isOfflineUserBlocked(userId: number): boolean {
	if (storageUnavailable) return blockedUsers.has(userId);
	try {
		return localStorage.getItem(`${KEY}:blocked:${userId}`) === 'true';
	} catch {
		return blockedUsers.has(userId);
	}
}

export function changeOfflineSession(userId: number, blocked: boolean): void {
	writeSession(
		userId,
		blocked,
		`${blocked ? 'blocked' : 'active'}:${userId}:${crypto.randomUUID()}`
	);
}

/** Apply a remote logout only to the session that sent it (or its shared tombstone). */
export function acceptOfflineSessionEnd(
	userId: number,
	previousVersion: string,
	version: string
): boolean {
	const current = read();
	if (
		!version.startsWith(`blocked:${userId}:`) ||
		(current !== previousVersion && current !== version)
	)
		return false;
	writeSession(userId, true, version);
	return true;
}

function writeSession(userId: number, blocked: boolean, version: string): void {
	if (blocked) blockedUsers.add(userId);
	else blockedUsers.delete(userId);
	fallback = version;
	try {
		if (blocked) localStorage.setItem(`${KEY}:blocked:${userId}`, 'true');
		else localStorage.removeItem(`${KEY}:blocked:${userId}`);
		localStorage.setItem(KEY, fallback);
		storageUnavailable = false;
	} catch {
		storageUnavailable = true;
	}
}

export function isCurrentOfflineSession(version: string): boolean {
	return read() === version;
}
