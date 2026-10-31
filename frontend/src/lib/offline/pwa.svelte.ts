import { version } from '$app/env';
import { SvelteSet } from 'svelte/reactivity';

interface InstallPromptEvent extends Event {
	prompt(): Promise<void>;
	userChoice: Promise<{ outcome: 'accepted' | 'dismissed' }>;
}

let installPrompt = $state<InstallPromptEvent | null>(null);
let updateAvailable = $state(false);
let initialized = false;
let updateError = $state<string | null>(null);
const dirtyForms = new SvelteSet<HTMLFormElement>();

export function getPwaState() {
	return {
		get canInstall() {
			return installPrompt !== null;
		},
		get updateAvailable() {
			return updateAvailable;
		},
		get updateError() {
			return updateError;
		}
	};
}

export function initializePwaLifecycle(): void {
	if (initialized || typeof window === 'undefined') return;
	initialized = true;
	window.addEventListener('input', (event) => {
		const form = event.target instanceof Element ? event.target.closest('form') : null;
		if (form) dirtyForms.add(form);
	});
	window.addEventListener('submit', (event) => {
		if (event.target instanceof HTMLFormElement) dirtyForms.delete(event.target);
	});
	window.addEventListener('beforeinstallprompt', (event) => {
		event.preventDefault();
		installPrompt = event as InstallPromptEvent;
	});
	window.addEventListener('appinstalled', () => {
		installPrompt = null;
	});

	if (!('serviceWorker' in navigator)) return;
	const announceVersion = () =>
		navigator.serviceWorker.controller?.postMessage?.({ type: 'CLIENT_VERSION', version });
	navigator.serviceWorker.addEventListener?.('controllerchange', announceVersion);
	window.addEventListener('focus', announceVersion);
	void navigator.serviceWorker.ready
		.then((registration) => {
			announceVersion();
			if (registration.waiting) updateAvailable = true;
			registration.addEventListener('updatefound', () => {
				const installing = registration.installing;
				installing?.addEventListener('statechange', () => {
					if (installing.state === 'installed' && navigator.serviceWorker.controller) {
						updateAvailable = true;
					}
				});
			});
		})
		.catch(() => undefined);
}

export async function promptInstall(): Promise<void> {
	if (!installPrompt) return;
	await installPrompt.prompt();
	await installPrompt.userChoice;
	installPrompt = null;
}

export async function activateWaitingServiceWorker(): Promise<void> {
	if (!('serviceWorker' in navigator)) return;
	if (
		[...dirtyForms].some((form) => form.isConnected) &&
		!window.confirm(
			'Ungespeicherte Eingaben gehen beim Aktualisieren verloren. Jetzt aktualisieren?'
		)
	)
		return;
	updateError = null;
	try {
		const registration = await navigator.serviceWorker.ready;
		if (!registration.waiting) return;
		await new Promise<void>((resolve, reject) => {
			const timeout = window.setTimeout(() => {
				navigator.serviceWorker.removeEventListener?.('controllerchange', activated);
				reject(
					new Error(
						'Die Aktualisierung konnte nicht abgeschlossen werden. Bitte versuche es erneut.'
					)
				);
			}, 15_000);
			const activated = () => {
				window.clearTimeout(timeout);
				resolve();
			};
			navigator.serviceWorker.addEventListener('controllerchange', activated, { once: true });
			registration.waiting?.postMessage('SKIP_WAITING');
		});
		window.location.reload();
	} catch (error) {
		updateError = error instanceof Error ? error.message : 'Die Aktualisierung ist fehlgeschlagen.';
	}
}
