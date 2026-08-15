import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const originalServiceWorker = Object.getOwnPropertyDescriptor(navigator, 'serviceWorker');

function setServiceWorker(value?: object) {
	if (value) {
		Object.defineProperty(navigator, 'serviceWorker', { configurable: true, value });
	} else {
		Reflect.deleteProperty(navigator, 'serviceWorker');
	}
}

describe('PWA lifecycle', () => {
	beforeEach(() => {
		vi.resetModules();
		setServiceWorker();
	});

	afterEach(() => {
		vi.restoreAllMocks();
		if (originalServiceWorker) {
			Object.defineProperty(navigator, 'serviceWorker', originalServiceWorker);
		} else {
			Reflect.deleteProperty(navigator, 'serviceWorker');
		}
	});

	it('captures, invokes and clears the browser install prompt', async () => {
		const module = await import('$lib/offline/pwa.svelte');
		module.initializePwaLifecycle();
		module.initializePwaLifecycle();

		const event = new Event('beforeinstallprompt', { cancelable: true }) as Event & {
			prompt: ReturnType<typeof vi.fn>;
			userChoice: Promise<{ outcome: 'accepted' }>;
		};
		event.prompt = vi.fn(async () => undefined);
		event.userChoice = Promise.resolve({ outcome: 'accepted' });
		window.dispatchEvent(event);

		expect(event.defaultPrevented).toBe(true);
		expect(module.getPwaState().canInstall).toBe(true);
		await module.promptInstall();
		expect(event.prompt).toHaveBeenCalledOnce();
		expect(module.getPwaState().canInstall).toBe(false);
		await expect(module.promptInstall()).resolves.toBeUndefined();

		window.dispatchEvent(event);
		window.dispatchEvent(new Event('appinstalled'));
		expect(module.getPwaState().canInstall).toBe(false);
	});

	it('detects an already waiting service worker', async () => {
		const registration = {
			waiting: { postMessage: vi.fn() },
			installing: null,
			addEventListener: vi.fn()
		};
		setServiceWorker({ ready: Promise.resolve(registration), controller: {} });
		const module = await import('$lib/offline/pwa.svelte');

		module.initializePwaLifecycle();
		await vi.waitFor(() => expect(module.getPwaState().updateAvailable).toBe(true));
		expect(registration.addEventListener).toHaveBeenCalledWith('updatefound', expect.any(Function));
	});

	it('detects a newly installed replacement worker', async () => {
		let updateFound: (() => void) | undefined;
		let stateChanged: (() => void) | undefined;
		const installing = {
			state: 'installing',
			addEventListener: vi.fn((_type: string, handler: () => void) => (stateChanged = handler))
		};
		const registration = {
			waiting: null,
			installing,
			addEventListener: vi.fn((_type: string, handler: () => void) => (updateFound = handler))
		};
		setServiceWorker({ ready: Promise.resolve(registration), controller: {} });
		const module = await import('$lib/offline/pwa.svelte');

		module.initializePwaLifecycle();
		await vi.waitFor(() => expect(updateFound).toBeTypeOf('function'));
		updateFound?.();
		installing.state = 'installed';
		stateChanged?.();

		expect(module.getPwaState().updateAvailable).toBe(true);
	});

	it('leaves workers alone when activation is unsupported or none is waiting', async () => {
		const moduleWithoutWorker = await import('$lib/offline/pwa.svelte');
		await expect(moduleWithoutWorker.activateWaitingServiceWorker()).resolves.toBeUndefined();

		vi.resetModules();
		setServiceWorker({
			ready: Promise.resolve({ waiting: null }),
			addEventListener: vi.fn()
		});
		const moduleWithoutUpdate = await import('$lib/offline/pwa.svelte');
		await expect(moduleWithoutUpdate.activateWaitingServiceWorker()).resolves.toBeUndefined();
	});

	it('asks a waiting worker to activate and waits for controllerchange', async () => {
		const waiting = { postMessage: vi.fn() };
		const serviceWorker = {
			ready: Promise.resolve({ waiting }),
			addEventListener: vi.fn((_type: string, handler: () => void) => handler())
		};
		setServiceWorker(serviceWorker);
		const module = await import('$lib/offline/pwa.svelte');

		await module.activateWaitingServiceWorker();

		expect(serviceWorker.addEventListener).toHaveBeenCalledWith(
			'controllerchange',
			expect.any(Function),
			{ once: true }
		);
		expect(waiting.postMessage).toHaveBeenCalledWith('SKIP_WAITING');
	});
});
