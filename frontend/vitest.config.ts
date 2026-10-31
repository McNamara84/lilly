import { defineConfig, mergeConfig } from 'vitest/config';
import viteConfig from './vite.config.ts';

export default mergeConfig(
	viteConfig,
	defineConfig({
		resolve: {
			conditions: ['browser']
		},
		test: {
			include: ['src/**/*.{test,spec}.{js,ts}', 'tests/**/*.{test,spec}.{js,ts}'],
			environment: 'jsdom',
			globals: true,
			setupFiles: ['./vitest-setup.ts'],
			coverage: {
				provider: 'v8',
				reporter: ['text', 'lcov', 'html'],
				include: ['src/**/*.{ts,svelte}', 'test-support/**/*.ts'],
				exclude: ['src/**/*.test.ts', 'src/**/*.spec.ts', 'src/app.html']
			}
		}
	})
);
