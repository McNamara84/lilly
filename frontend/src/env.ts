import { defineEnvVars } from '@sveltejs/kit/env';

// Operator settings are optional, private, and read when the server starts.
const optionalText = { schema: (value: string | undefined) => value ?? '' };

export const variables = defineEnvVars({
	IMPRINT_OPERATOR_NAME: optionalText,
	IMPRINT_ADDRESS: optionalText,
	IMPRINT_EMAIL: optionalText,
	IMPRINT_PHONE: optionalText,
	IMPRINT_RESPONSIBLE_PERSON: optionalText,
	IMPRINT_ADDITIONAL: optionalText,
	IMPRINT_LAST_UPDATED: optionalText
});
