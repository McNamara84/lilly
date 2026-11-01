/* eslint-disable no-console -- Test CLI progress and diagnostics. */
import { spawn } from 'node:child_process';
import { cp, mkdir } from 'node:fs/promises';
import http from 'node:http';
import path from 'node:path';
import { randomUUID } from 'node:crypto';
import { once } from 'node:events';

const cwd = process.cwd();
const backend = new URL(process.env.PLAYWRIGHT_BASE_URL || 'http://localhost');
const output = path.resolve('.pwa-builds', randomUUID());
const token = randomUUID();
const children = [];
let active = 'a';
let failAssets = false;
const servers = {};

function run(args, env = process.env) {
	return new Promise((resolve, reject) => {
		const child = spawn(process.execPath, args, { cwd, env, stdio: 'inherit', windowsHide: true });
		children.push(child);
		child.on('error', reject);
		child.on('exit', (code) =>
			code === 0 ? resolve() : reject(new Error(`Command failed (${code}): ${args[0]}`))
		);
	});
}

async function freePort() {
	const listener = http.createServer();
	listener.listen(0, '127.0.0.1');
	await once(listener, 'listening');
	const port = listener.address().port;
	await new Promise((resolve) => listener.close(resolve));
	return port;
}

const proxy = http.createServer((request, response) => {
	if (request.url === '/__pwa_test__/switch') {
		if (request.method !== 'POST' || request.headers['x-pwa-test-token'] !== token) {
			response.writeHead(403).end();
			return;
		}
		let body = '';
		request.on('data', (chunk) => {
			body += chunk;
		});
		request.on('end', () => {
			try {
				const command = JSON.parse(body);
				if (!['a', 'b'].includes(command.build)) throw new Error('Invalid build');
				active = command.build;
				failAssets = command.failAssets === true;
				response
					.writeHead(200, { 'Content-Type': 'application/json' })
					.end(JSON.stringify({ build: active }));
			} catch {
				response.writeHead(400).end();
			}
		});
		return;
	}
	if (failAssets && request.url.startsWith('/_app/immutable/')) {
		response.writeHead(503).end('Interrupted update fixture');
		return;
	}
	const business = request.url.startsWith('/api/') || request.url.startsWith('/media/');
	const target = new URL(request.url, business ? backend : servers[active]);
	const headers = { ...request.headers, host: target.host };
	// The isolated proxy deliberately represents the test application's configured origin.
	if (business && headers.origin) headers.origin = backend.origin;
	const upstream = http.request(target, { method: request.method, headers }, (incoming) => {
		response.writeHead(incoming.statusCode, { ...incoming.headers, 'cache-control': 'no-store' });
		incoming.pipe(response);
	});
	upstream.on('error', () => response.writeHead(502).end());
	request.pipe(upstream);
});

try {
	await mkdir(output, { recursive: true });
	proxy.listen(0, '127.0.0.1');
	await once(proxy, 'listening');
	const origin = `http://127.0.0.1:${proxy.address().port}`;
	for (const build of ['a', 'b']) {
		await run(['node_modules/vite/bin/vite.js', 'build'], {
			...process.env,
			LILLY_BUILD_VERSION: `pwa-test-${build}`
		});
		const directory = path.join(output, build);
		await cp(path.join(cwd, 'build'), directory, { recursive: true });
		const port = await freePort();
		servers[build] = `http://127.0.0.1:${port}`;
		const child = spawn(process.execPath, [path.join(directory, 'index.js')], {
			cwd,
			env: { ...process.env, PORT: String(port), HOST: '127.0.0.1', ORIGIN: origin },
			stdio: ['ignore', 'pipe', 'inherit'],
			windowsHide: true
		});
		children.push(child);
		await new Promise((resolve, reject) => {
			child.once('error', reject);
			child.once('exit', (code) => reject(new Error(`Build ${build} exited (${code})`)));
			child.stdout.on('data', (chunk) => {
				if (chunk.toString().includes('Listening')) resolve();
			});
		});
	}
	await run(
		[
			'node_modules/@playwright/test/cli.js',
			'test',
			'e2e/pwa-update.spec.ts',
			'--output=test-results-update',
			'--workers=1',
			'--project=chromium',
			'--project=firefox',
			'--project=webkit'
		],
		{ ...process.env, PLAYWRIGHT_BASE_URL: origin, PWA_UPDATE_CONTROL_TOKEN: token }
	);
} catch (error) {
	console.error(error.message);
	process.exitCode = 1;
} finally {
	for (const child of children) if (child.exitCode === null) child.kill();
	proxy.closeAllConnections();
	proxy.close();
}
