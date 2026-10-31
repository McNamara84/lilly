import { readFileSync } from 'node:fs';
import type { APIResponse, Response } from '@playwright/test';
import { createContractValidator, type ApiContract } from '../test-support/api-contract';

const contract = JSON.parse(
	readFileSync(new URL('../../docs/openapi.json', import.meta.url), 'utf8')
) as ApiContract;
export const validateApiPayload = createContractValidator(contract);

export async function assertApiResponse(response: APIResponse | Response, method?: string) {
	const verb = method ?? ('request' in response ? response.request().method() : 'GET');
	const contentType = response.headers()['content-type'] ?? '';
	const bytes = await response.body();
	let body: unknown;
	if (bytes.length) {
		if (contentType.startsWith('application/json')) {
			try {
				body = JSON.parse(bytes.toString('utf8'));
			} catch {
				throw new Error(`Invalid JSON response: ${verb} ${new URL(response.url()).pathname}`);
			}
		} else {
			body = bytes.toString('latin1');
		}
	}
	validateApiPayload(response.url(), verb, response.status(), contentType, body);
}
