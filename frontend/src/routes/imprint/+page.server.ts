import * as env from '$app/env/private';
import { readImprintConfig } from '#lib/imprint.js';
import type { PageServerLoad } from './$types';

export const load: PageServerLoad = () => ({
	imprint: readImprintConfig(env)
});
