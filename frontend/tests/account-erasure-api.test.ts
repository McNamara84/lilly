import { beforeEach, describe, expect, it, vi } from 'vitest';
import {
	availableOAuthMethods,
	cancelAccountDeletion,
	fetchAccountDeletionOptions,
	fetchAccountDeletionStatus,
	reauthenticateWithPassword,
	requestAccountDeletion
} from '../src/lib/api/account-erasure';

const fetchMock = vi.fn();
vi.stubGlobal('fetch', fetchMock);

describe('account-erasure API', () => {
	beforeEach(() => fetchMock.mockReset());

	it('loads options and the recovery-scoped status with credentials', async () => {
		fetchMock
			.mockResolvedValueOnce({ ok: true, json: async () => ({ password: true }) })
			.mockResolvedValueOnce({
				ok: true,
				json: async () => ({ status: 'scheduled', can_cancel: true })
			});

		await fetchAccountDeletionOptions();
		await fetchAccountDeletionStatus();

		expect(fetchMock).toHaveBeenNthCalledWith(1, '/api/v1/me/account-deletion/options', {
			credentials: 'same-origin'
		});
		expect(fetchMock).toHaveBeenNthCalledWith(2, '/api/v1/me/account-deletion', {
			credentials: 'same-origin'
		});
	});

	it('sends password reauthentication and the exact confirmation separately', async () => {
		fetchMock
			.mockResolvedValueOnce({ ok: true, json: async () => ({ message: 'ok' }) })
			.mockResolvedValueOnce({
				ok: true,
				json: async () => ({ status: 'scheduled' })
			});

		await reauthenticateWithPassword('very secret');
		await requestAccountDeletion('KONTO LÖSCHEN');

		expect(fetchMock).toHaveBeenNthCalledWith(
			1,
			'/api/v1/auth/reauth/password',
			expect.objectContaining({
				method: 'POST',
				body: JSON.stringify({ password: 'very secret' })
			})
		);
		expect(fetchMock).toHaveBeenNthCalledWith(
			2,
			'/api/v1/me/account-deletion',
			expect.objectContaining({
				method: 'POST',
				body: JSON.stringify({ confirmation: 'KONTO LÖSCHEN' })
			})
		);
	});

	it('cancels through the recovery endpoint and preserves stable errors', async () => {
		fetchMock
			.mockResolvedValueOnce({ ok: true, json: async () => ({ message: 'cancelled' }) })
			.mockResolvedValueOnce({
				ok: false,
				status: 403,
				json: async () => ({
					error: 'Recovery required',
					code: 'ACCOUNT_DELETION_RECOVERY_REQUIRED'
				})
			});

		await cancelAccountDeletion();
		expect(fetchMock).toHaveBeenNthCalledWith(1, '/api/v1/me/account-deletion', {
			method: 'DELETE',
			credentials: 'same-origin'
		});
		await expect(fetchAccountDeletionStatus()).rejects.toMatchObject({
			message: 'Recovery required',
			code: 'ACCOUNT_DELETION_RECOVERY_REQUIRED',
			status: 403
		});
	});

	it('returns only linked OAuth reauthentication methods', () => {
		expect(
			availableOAuthMethods({
				recent_authentication: false,
				password: false,
				google: true,
				github: false,
				confirmation_phrase: 'KONTO LÖSCHEN',
				grace_days: 7
			})
		).toEqual(['google']);
	});

	it('uses the stable fallback for a non-JSON error response', async () => {
		fetchMock.mockResolvedValueOnce({
			ok: false,
			status: 502,
			json: async () => {
				throw new SyntaxError('invalid JSON');
			}
		});

		await expect(fetchAccountDeletionOptions()).rejects.toMatchObject({
			message: 'Ein unerwarteter Fehler ist aufgetreten.',
			status: 502
		});
	});
});
