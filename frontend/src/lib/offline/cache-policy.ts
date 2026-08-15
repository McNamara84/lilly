export function isPrivateCachePath(pathname: string): boolean {
	return (
		pathname.startsWith('/api/') ||
		(pathname.startsWith('/media/') && !pathname.startsWith('/media/covers/'))
	);
}

export function isCacheableNavigationPath(pathname: string): boolean {
	return /^(?:\/$|\/login\/?$|\/collection(?:\/add)?\/?$)/.test(pathname);
}
