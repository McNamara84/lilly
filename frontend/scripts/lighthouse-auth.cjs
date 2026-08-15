module.exports = async (browser, context) => {
	const target = new URL(context.url);
	const page = await browser.newPage();
	try {
		await page.goto(target.origin, { waitUntil: 'domcontentloaded' });
		const result = await page.evaluate(async () => {
			const response = await fetch('/api/v1/auth/login', {
				method: 'POST',
				headers: { 'Content-Type': 'application/json' },
				credentials: 'same-origin',
				body: JSON.stringify({
					email: 'e2e-worker-0@lilly.app',
					password: 'e2e-worker-password'
				})
			});
			return { ok: response.ok, status: response.status };
		});
		if (!result.ok) throw new Error(`Lighthouse login failed with HTTP ${result.status}`);
	} finally {
		await page.close();
	}
};
