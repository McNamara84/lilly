interface InstallPromptEvent extends Event {
	prompt(): Promise<void>;
	userChoice: Promise<{ outcome: 'accepted' | 'dismissed' }>;
}

let installPrompt = $state<InstallPromptEvent | null>(null);
let updateAvailable = $state(false);
let initialized = false;

export function getPwaState() {
	return {
		get canInstall() {
			return installPrompt !== null;
		},
		get updateAvailable() {
			return updateAvailable;
		}
	};
}

export function initializePwaLifecycle(): void {
	if (initialized || typeof window === 'undefined') return;
	initialized = true;
	window.addEventListener('beforeinstallprompt', (event) => {
		event.preventDefault();
		installPrompt = event as InstallPromptEvent;
	});
	window.addEventListener('appinstalled', () => {
		installPrompt = null;
	});

	if (!('serviceWorker' in navigator)) return;
	void navigator.serviceWorker.ready.then((registration) => {
		if (registration.waiting) updateAvailable = true;
		registration.addEventListener('updatefound', () => {
			const installing = registration.installing;
			installing?.addEventListener('statechange', () => {
				if (installing.state === 'installed' && navigator.serviceWorker.controller) {
					updateAvailable = true;
				}
			});
		});
	});
}

export async function promptInstall(): Promise<void> {
	if (!installPrompt) return;
	await installPrompt.prompt();
	await installPrompt.userChoice;
	installPrompt = null;
}

export async function activateWaitingServiceWorker(): Promise<void> {
	if (!('serviceWorker' in navigator)) return;
	const registration = await navigator.serviceWorker.ready;
	if (!registration.waiting) return;
	await new Promise<void>((resolve) => {
		navigator.serviceWorker.addEventListener('controllerchange', () => resolve(), { once: true });
		registration.waiting?.postMessage('SKIP_WAITING');
	});
	window.location.reload();
}
