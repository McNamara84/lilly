import { env } from '$env/dynamic/private';
import { readImprintConfig } from '$lib/imprint';
import type { PageServerLoad } from './$types';

export const load: PageServerLoad = () => ({
	imprint: readImprintConfig(env)
});
