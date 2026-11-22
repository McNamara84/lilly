import { SvelteURL } from 'svelte/reactivity';

const currentURL = new SvelteURL('http://localhost/collection');

export const collectionTestPage = {
	get url() {
		return currentURL;
	}
};

export function setCollectionTestURL(path = '/collection') {
	currentURL.href = new SvelteURL(path, 'http://localhost').href;
}
