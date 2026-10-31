import { describe, expect, it } from 'vitest';
import { createContractValidator, type ApiContract } from '../test-support/api-contract';

const contract: ApiContract = {
	components: {
		schemas: {
			Entry: {
				type: 'object',
				required: ['id', 'date'],
				properties: {
					id: { type: 'integer', format: 'int64' },
					date: { type: ['string', 'null'], format: 'date' },
					updated: { type: 'string', format: 'local-date-time' }
				}
			}
		}
	},
	paths: {
		'/api/v1/entries/{id}': {
			get: {
				responses: {
					'200': {
						content: { 'application/json': { schema: { $ref: '#/components/schemas/Entry' } } }
					}
				}
			},
			delete: { responses: { '204': {} } }
		},
		'/api/v1/entries/stats': {
			get: {
				responses: { '200': { content: { 'application/json': { schema: { type: 'integer' } } } } }
			}
		},
		'/api/v1/photo': {
			get: {
				responses: {
					'200': { content: { 'image/jpeg': { schema: { type: 'string', format: 'binary' } } } }
				}
			}
		}
	}
};

describe('OpenAPI response validation', () => {
	it('resolves shared schemas, accepts nullable dates and ignores query strings', () => {
		const validate = createContractValidator(contract);
		for (const date of [null, '2024-02-29']) {
			validate(
				'https://lilly.test/api/v1/entries/1?page=2',
				'GET',
				200,
				'application/json; charset=utf-8',
				{
					id: 1,
					date,
					updated: '2026-10-31T12:30:00.123'
				}
			);
		}
	});
	it('prefers a literal route to a placeholder', () => {
		createContractValidator(contract)('/api/v1/entries/stats', 'get', 200, 'application/json', 3);
	});
	it.each([
		['/api/v1/unknown', 'GET', 200, 'application/json', {}, 'operation'],
		['/api/v1/entries/', 'GET', 200, 'application/json', {}, 'operation'],
		['/api/v1/entries/1', 'POST', 200, 'application/json', {}, 'operation'],
		['/api/v1/entries/1', 'GET', 404, 'application/json', {}, 'status'],
		['/api/v1/entries/1', 'GET', 200, 'text/html', '', 'content type'],
		['/api/v1/entries/1', 'GET', 200, 'application/json', {}, 'mismatch'],
		[
			'/api/v1/entries/1',
			'GET',
			200,
			'application/json',
			{ id: 1, date: '2025-02-29' },
			'mismatch'
		],
		[
			'/api/v1/entries/1',
			'GET',
			200,
			'application/json',
			{ id: 1, date: null, updated: 'wrong' },
			'mismatch'
		],
		['/api/v1/entries/1', 'DELETE', 204, '', { unexpected: true }, 'body']
	])('rejects incompatible response %#', (url, method, status, type, body, error) => {
		expect(() =>
			createContractValidator(contract)(
				url as string,
				method as string,
				status as number,
				type as string,
				body
			)
		).toThrow(error as string);
	});
	it('validates empty and binary responses', () => {
		const validate = createContractValidator(contract);
		validate('/api/v1/entries/1', 'DELETE', 204, '');
		validate('/api/v1/entries/1', 'DELETE', 204, '', '');
		validate('/api/v1/photo', 'GET', 200, 'image/jpeg', '\xff\xd8');
	});
	it('does not expose private response data in diagnostics', () => {
		let thrown: unknown;
		try {
			createContractValidator(contract)('/api/v1/entries/1', 'GET', 200, 'application/json', {
				id: 'secret-token',
				date: null
			});
		} catch (error) {
			thrown = error;
		}
		expect(thrown).toBeInstanceOf(Error);
		expect(String(thrown)).toMatch(/mismatch/);
		expect(String(thrown)).not.toContain('secret-token');
	});
});
