export function isNetworkFailure(error: unknown): boolean {
	if (typeof navigator !== 'undefined' && navigator.onLine === false) return true;
	if (error instanceof TypeError) return true;
	return error instanceof DOMException && error.name === 'NetworkError';
}
