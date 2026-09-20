import adapter from '@sveltejs/adapter-node';

/** @type {import('@sveltejs/kit').Config} */
const config = {
	kit: {
		adapter: adapter(),
		// SvelteKit adds per-response nonces (or hashes for prerendered pages) to its own inline
		// bootstrap script, so `script-src` needs no `'unsafe-inline'`. Inline `style` attributes are
		// used throughout the UI, which is why `style-src` still allows them; that cannot execute code.
		csp: {
			mode: 'auto',
			directives: {
				'default-src': ['self'],
				'script-src': ['self'],
				'style-src': ['self', 'unsafe-inline'],
				'img-src': ['self', 'data:', 'blob:'],
				'font-src': ['self'],
				'connect-src': ['self'],
				'worker-src': ['self'],
				'manifest-src': ['self'],
				'object-src': ['none'],
				'base-uri': ['self'],
				'form-action': ['self'],
				'frame-ancestors': ['self']
			}
		}
	}
};

export default config;
