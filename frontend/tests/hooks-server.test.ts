import { describe, it, expect } from 'vitest';
import type { RequestEvent } from '@sveltejs/kit';
import { handle, securityHeaders } from '../src/hooks.server';

async function run(response: Response) {
	return handle({
		event: {} as RequestEvent,
		resolve: async () => response
	});
}

describe('security headers hook', () => {
	it('adds the defensive headers to every response', async () => {
		const response = await run(new Response('ok'));

		for (const [name, value] of Object.entries(securityHeaders)) {
			expect(response.headers.get(name)).toBe(value);
		}
		expect(response.headers.get('X-Content-Type-Options')).toBe('nosniff');
		expect(response.headers.get('X-Frame-Options')).toBe('SAMEORIGIN');
	});

	it('allows camera capture only for the app itself and denies other sensitive features', async () => {
		const policy = (await run(new Response('ok'))).headers.get('Permissions-Policy') ?? '';

		expect(policy).toContain('camera=(self)');
		expect(policy).toContain('microphone=()');
		expect(policy).toContain('geolocation=()');
	});

	it('keeps headers that were already set by a route', async () => {
		const response = await run(
			new Response('ok', { headers: { 'Referrer-Policy': 'no-referrer' } })
		);

		expect(response.headers.get('Referrer-Policy')).toBe('no-referrer');
		expect(response.headers.get('X-Content-Type-Options')).toBe('nosniff');
	});
});
