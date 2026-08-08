import type { CollectionQueryParams } from '$lib/api/collection';
import { CONDITION_GRADES } from '$lib/collection/conditions';

export const COLLECTION_SORTS = [
	'issue_number',
	'series',
	'condition',
	'title',
	'author',
	'added'
] as const;

export const COLLECTION_STATUSES = ['owned', 'duplicate', 'wanted', 'missing'] as const;
export { CONDITION_GRADES };

function trimmed(value: string | null | undefined): string | undefined {
	const result = value?.trim();
	return result ? result : undefined;
}

function positiveInteger(value: string | null | undefined): number | undefined {
	if (!value || !/^\d+$/.test(value)) return undefined;
	const parsed = Number(value);
	return Number.isSafeInteger(parsed) && parsed > 0 ? parsed : undefined;
}

function isOneOf<T extends readonly string[]>(
	value: string | null | undefined,
	allowed: T
): value is T[number] {
	return typeof value === 'string' && allowed.includes(value as T[number]);
}

export function normalizeCollectionQuery(params: CollectionQueryParams): CollectionQueryParams {
	const series_slug = trimmed(params.series_slug);
	let status = isOneOf(params.status, COLLECTION_STATUSES) ? params.status : undefined;
	const missingRequested = status === 'missing';
	const issue_number =
		params.issue_number && Number.isSafeInteger(params.issue_number) && params.issue_number > 0
			? params.issue_number
			: undefined;
	let condition = isOneOf(params.condition, CONDITION_GRADES) ? params.condition : undefined;
	const title = trimmed(params.title);
	const author = trimmed(params.author);
	let sort = isOneOf(params.sort, COLLECTION_SORTS) ? params.sort : undefined;
	const sort_dir = params.sort_dir === 'desc' ? 'desc' : undefined;
	const page =
		params.page && Number.isSafeInteger(params.page) && params.page > 1 ? params.page : undefined;
	const per_page =
		params.per_page && Number.isSafeInteger(params.per_page) && params.per_page > 0
			? Math.min(params.per_page, 100)
			: undefined;

	if (status === 'missing' && !series_slug) status = undefined;
	if (missingRequested) {
		condition = undefined;
		if (sort === 'condition' || sort === 'added') sort = undefined;
	}

	return {
		...(series_slug ? { series_slug } : {}),
		...(status ? { status } : {}),
		...(issue_number ? { issue_number } : {}),
		...(condition ? { condition } : {}),
		...(title ? { title } : {}),
		...(author ? { author } : {}),
		...(sort && sort !== 'issue_number' ? { sort } : {}),
		...(sort_dir ? { sort_dir } : {}),
		...(page ? { page } : {}),
		...(per_page ? { per_page } : {})
	};
}

export function parseCollectionQuery(searchParams: URLSearchParams): CollectionQueryParams {
	return normalizeCollectionQuery({
		series_slug: searchParams.get('series_slug') ?? undefined,
		status: searchParams.get('status') ?? undefined,
		issue_number: positiveInteger(searchParams.get('issue_number')),
		condition: searchParams.get('condition') ?? undefined,
		title: searchParams.get('title') ?? undefined,
		author: searchParams.get('author') ?? undefined,
		sort: searchParams.get('sort') ?? undefined,
		sort_dir: searchParams.get('sort_dir') ?? undefined,
		page: positiveInteger(searchParams.get('page')),
		per_page: positiveInteger(searchParams.get('per_page'))
	});
}

export function serializeCollectionQuery(params: CollectionQueryParams): URLSearchParams {
	const normalized = normalizeCollectionQuery(params);
	const searchParams = new URLSearchParams();

	for (const key of [
		'series_slug',
		'status',
		'issue_number',
		'condition',
		'title',
		'author',
		'sort',
		'sort_dir',
		'page',
		'per_page'
	] as const) {
		const value = normalized[key];
		if (value !== undefined) searchParams.set(key, String(value));
	}

	return searchParams;
}

export function collectionQueryHref(params: CollectionQueryParams): string {
	const query = serializeCollectionQuery(params).toString();
	return query ? `/collection?${query}` : '/collection';
}

export function countActiveCollectionFilters(params: CollectionQueryParams): number {
	const normalized = normalizeCollectionQuery(params);
	return ['series_slug', 'status', 'issue_number', 'condition', 'title', 'author'].filter(
		(key) => normalized[key as keyof CollectionQueryParams] !== undefined
	).length;
}
