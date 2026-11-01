// Authenticate without navigating the browser or warming application resources.
function sessionCookies(headers, origin) {
	return headers.getSetCookie().map((header) => {
		const [pair, ...attributes] = header.split(';');
		const equals = pair.indexOf('=');
		if (equals < 1) throw new Error('Invalid authentication cookie');
		const cookie = { name: pair.slice(0, equals), value: pair.slice(equals + 1), url: origin };
		for (const attribute of attributes) {
			const [name, value] = attribute.trim().split('=');
			if (name.toLowerCase() === 'httponly') cookie.httpOnly = true;
			if (name.toLowerCase() === 'secure') cookie.secure = true;
			if (name.toLowerCase() === 'samesite')
				cookie.sameSite = value[0].toUpperCase() + value.slice(1).toLowerCase();
		}
		return cookie;
	});
}

async function authenticate(page, origin, request = fetch) {
	const response = await request(`${origin}/api/v1/auth/login`, {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify({
			email: process.env.LIGHTHOUSE_AUTH_EMAIL || 'e2e-performance@lilly.app',
			password: process.env.LIGHTHOUSE_AUTH_PASSWORD || 'e2e-pwa-password'
		})
	});
	if (!response.ok) throw new Error(`Lighthouse login failed with HTTP ${response.status}`);
	const cookies = sessionCookies(response.headers, origin);
	if (!cookies.length) throw new Error('Lighthouse login returned no session cookies');
	await page.setCookie(...cookies);
	return cookies;
}

module.exports = { authenticate, sessionCookies };
