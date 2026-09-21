export interface ImprintConfig {
	configured: boolean;
	operatorName: string;
	addressLines: string[];
	email: string;
	phone: string;
	responsiblePerson: string;
	additionalParagraphs: string[];
	lastUpdated: string;
}

type ImprintEnv = Record<string, string | undefined>;

function text(env: ImprintEnv, key: string, maxLength = 500): string {
	return (env[key] ?? '').trim().slice(0, maxLength);
}

function lines(env: ImprintEnv, key: string, maxLines: number, maxLength = 500): string[] {
	return text(env, key, maxLength * maxLines)
		.split('|')
		.map((line) => line.trim().slice(0, maxLength))
		.filter(Boolean)
		.slice(0, maxLines);
}

/**
 * Read the operator's legal notice from `IMPRINT_*` environment variables so that each
 * self-hosted installation can publish its own imprint without changing the source code.
 * Multi-line values use `|` as the line separator because `.env` files are single-line.
 */
export function readImprintConfig(env: ImprintEnv): ImprintConfig {
	const operatorName = text(env, 'IMPRINT_OPERATOR_NAME');
	const email = text(env, 'IMPRINT_EMAIL');
	const addressLines = lines(env, 'IMPRINT_ADDRESS', 6);

	return {
		configured: Boolean(operatorName && email && addressLines.length > 0),
		operatorName,
		addressLines,
		email,
		phone: text(env, 'IMPRINT_PHONE', 64),
		responsiblePerson: text(env, 'IMPRINT_RESPONSIBLE_PERSON'),
		additionalParagraphs: lines(env, 'IMPRINT_ADDITIONAL', 6, 1000),
		lastUpdated: text(env, 'IMPRINT_LAST_UPDATED', 32)
	};
}
