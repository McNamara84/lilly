import { changeOfflineSession, offlineSessionVersion } from '#lib/offline/session.js';

/** Model the sending tab persisting its tombstone before delivering the notification. */
export function sessionEndMessage(type: 'logout' | 'account-deletion', userId: number) {
	const previousVersion = offlineSessionVersion();
	changeOfflineSession(userId, true);
	return {
		data: {
			type,
			user_id: userId,
			previous_version: previousVersion,
			session_version: offlineSessionVersion()
		}
	} as MessageEvent;
}
