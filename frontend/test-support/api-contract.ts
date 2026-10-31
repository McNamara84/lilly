import Ajv2020, { type AnySchema, type ValidateFunction } from 'ajv/dist/2020.js';
import addFormats from 'ajv-formats';

type ResponseContract = { content?: Record<string, { schema: AnySchema }> };
type Operation = { responses: Record<string, ResponseContract> };
export type ApiContract = {
	paths: Record<string, Record<string, Operation>>;
	components: Record<string, unknown>;
};

/** Validate real HTTP responses without including private response bodies in failures. */
export function createContractValidator(contract: ApiContract) {
	const ajv = new Ajv2020({ strict: false, allErrors: true });
	addFormats(ajv);
	ajv.addFormat('int32', { type: 'number', validate: Number.isInteger });
	ajv.addFormat('int64', { type: 'number', validate: Number.isSafeInteger });
	ajv.addFormat('float', { type: 'number', validate: Number.isFinite });
	ajv.addFormat('double', { type: 'number', validate: Number.isFinite });
	ajv.addFormat('binary', true);
	ajv.addFormat('local-date-time', /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?$/);
	const validators = new Map<string, ValidateFunction>();
	// Literal paths must take precedence over placeholders such as /collection/{id}.
	const paths = Object.keys(contract.paths).sort(
		(a, b) => (a.match(/\{/g)?.length ?? 0) - (b.match(/\{/g)?.length ?? 0)
	);
	return (url: string, method: string, status: number, contentType: string, body?: unknown) => {
		const pathname = new URL(url, 'http://contract.test').pathname;
		const segments = pathname.split('/');
		const template = paths.find((candidate) => {
			const parts = candidate.split('/');
			return (
				parts.length === segments.length &&
				parts.every((part, i) =>
					part.startsWith('{') ? segments[i].length > 0 : part === segments[i]
				)
			);
		});
		const label = `${method.toUpperCase()} ${template ?? pathname} ${status}`;
		const operation = template && contract.paths[template][method.toLowerCase()];
		if (!operation) throw new Error(`Undocumented API operation: ${label}`);
		const response = operation.responses[String(status)];
		if (!response) throw new Error(`Undocumented response status: ${label}`);
		if (!response.content) {
			if (body !== undefined && body !== '') throw new Error(`Unexpected response body: ${label}`);
			return;
		}
		const mediaType = contentType.split(';')[0].trim().toLowerCase();
		const media = response.content[mediaType];
		if (!media) throw new Error(`Undocumented response content type: ${label} ${mediaType}`);
		const key = `${label} ${mediaType}`;
		let validate = validators.get(key);
		if (!validate) {
			validate = ajv.compile({ ...(media.schema as object), components: contract.components });
			validators.set(key, validate);
		}
		if (!validate(body)) {
			const errors = validate.errors?.map((error) => `${error.instancePath}: ${error.message}`);
			throw new Error(`API contract mismatch: ${label}\n${errors?.join('\n')}`);
		}
	};
}
