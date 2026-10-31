import type {
	CollectionEntry,
	CollectionQueryParams,
	PaginatedCollection
} from '#lib/api/collection.js';
import { listStoredCollectionEntries, listStoredIssues, listStoredSeries } from './database';
export { listConflicts, listMutations } from './database';
export {
	queueCollectionCreate,
	queueCollectionUpdate,
	synchronizeCollection,
	acceptConflictServerVersion,
	reapplyConflictLocalVersion
} from './queue';

function conditionRank(condition: string | null): number {
	return condition ? Number(condition.slice(1)) : Number.POSITIVE_INFINITY;
}

function compareEntries(
	left: CollectionEntry,
	right: CollectionEntry,
	sort: string,
	direction: 'asc' | 'desc'
): number {
	let comparison: number;
	switch (sort) {
		case 'issue_number':
			comparison = left.issue_number - right.issue_number;
			break;
		case 'condition':
			comparison = conditionRank(left.condition_grade) - conditionRank(right.condition_grade);
			break;
		case 'title':
			comparison = left.title.localeCompare(right.title, 'de');
			break;
		case 'added':
			comparison = (left.created_at ?? '').localeCompare(right.created_at ?? '');
			break;
		default:
			comparison = left.series_name.localeCompare(right.series_name, 'de');
			if (comparison === 0) comparison = left.issue_number - right.issue_number;
	}
	if (comparison === 0) comparison = left.id - right.id;
	return direction === 'desc' ? -comparison : comparison;
}

export function filterLocalCollection(
	entries: CollectionEntry[],
	params: CollectionQueryParams,
	authorsByIssue: Map<number, string[]> = new Map()
): PaginatedCollection {
	const normalizedSearch = params.q?.trim().toLocaleLowerCase('de');
	const normalizedTitle = params.title?.trim().toLocaleLowerCase('de');
	const normalizedAuthor = params.author?.trim().toLocaleLowerCase('de');
	let filtered = entries.filter((entry) => {
		if (params.series_slug && entry.series_slug !== params.series_slug) return false;
		if (params.issue_id && entry.issue_id !== params.issue_id) return false;
		if (params.status && params.status !== 'missing' && entry.status !== params.status)
			return false;
		if (params.issue_number && entry.issue_number !== params.issue_number) return false;
		if (params.condition && entry.condition_grade !== params.condition) return false;
		if (
			params.condition_min &&
			conditionRank(entry.condition_grade) < conditionRank(params.condition_min)
		) {
			return false;
		}
		if (
			params.condition_max &&
			conditionRank(entry.condition_grade) > conditionRank(params.condition_max)
		) {
			return false;
		}
		if (normalizedTitle && !entry.title.toLocaleLowerCase('de').includes(normalizedTitle)) {
			return false;
		}
		const authors = authorsByIssue.get(entry.issue_id) ?? [];
		if (
			normalizedAuthor &&
			!authors.some((author) => author.toLocaleLowerCase('de').includes(normalizedAuthor))
		) {
			return false;
		}
		if (
			normalizedSearch &&
			!entry.title.toLocaleLowerCase('de').includes(normalizedSearch) &&
			!authors.some((author) => author.toLocaleLowerCase('de').includes(normalizedSearch))
		) {
			return false;
		}
		return true;
	});

	const direction = params.sort_dir === 'desc' ? 'desc' : 'asc';
	filtered = filtered.sort((left, right) =>
		compareEntries(left, right, params.sort ?? 'series', direction)
	);
	const total = filtered.length;
	const page = Math.max(1, params.page ?? 1);
	const perPage = Math.min(100, Math.max(1, params.per_page ?? 50));
	const offset = (page - 1) * perPage;
	return { data: filtered.slice(offset, offset + perPage), page, per_page: perPage, total };
}

export async function readLocalCollection(
	userId: number,
	params: CollectionQueryParams
): Promise<PaginatedCollection> {
	const [entries, issues] = await Promise.all([
		listStoredCollectionEntries(userId),
		listStoredIssues(userId)
	]);
	const authors = new Map(issues.map((issue) => [issue.id, issue.authors]));
	if (params.status !== 'missing') return filterLocalCollection(entries, params, authors);

	const series = await listStoredSeries(userId);
	const seriesById = new Map(series.map((item) => [item.id, item]));
	const ownedIssueIds = new Set(entries.map((entry) => entry.issue_id));
	const missing = issues
		.filter((issue) => !ownedIssueIds.has(issue.id))
		.map((issue): CollectionEntry => {
			const itemSeries = seriesById.get(issue.series_id);
			return {
				id: 0,
				issue_id: issue.id,
				issue_number: issue.issue_number,
				title: issue.title,
				series_id: issue.series_id,
				series_name: itemSeries?.name ?? '',
				series_slug: itemSeries?.slug ?? '',
				cover_url: issue.cover_url,
				cover_local_path: issue.cover_local_path,
				copy_number: null,
				edition_label: null,
				condition_grade: null,
				status: 'missing',
				notes: null,
				revision: null,
				created_at: null,
				updated_at: null
			};
		});
	return filterLocalCollection(missing, params, authors);
}
