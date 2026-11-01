import { vi } from 'vitest';

export function mockWaitingUpdate() {
	const postMessage = vi.fn();
	vi.stubGlobal('navigator', {
		serviceWorker: {
			ready: Promise.resolve({ waiting: { postMessage }, addEventListener: vi.fn() }),
			addEventListener: vi.fn((_type, activated) => activated())
		}
	});
	const confirm = vi.spyOn(window, 'confirm').mockReturnValue(false);
	return { postMessage, confirm };
}
