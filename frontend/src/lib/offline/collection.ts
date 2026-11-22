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

const textOrder = new Intl.Collator('de', { sensitivity: 'base' });

function compareText(left: string, right: string): number {
	// The database's text keys ignore case, accents and trailing ASCII spaces.
	return textOrder.compare(left.replace(/ +$/u, ''), right.replace(/ +$/u, ''));
}

function compareEntries(
	left: CollectionEntry,
	right: CollectionEntry,
	sort: string,
	direction: 'asc' | 'desc',
	firstAuthors: ReadonlyMap<number, string>
): number {
	let comparison: number;
	switch (sort) {
		case 'series':
			comparison = compareText(left.series_name, right.series_name);
			break;
		case 'condition':
			// FIELD(NULL, ...) is zero; named grades start at one.
			comparison =
				(left.condition_grade === null ? 0 : conditionRank(left.condition_grade) + 1) -
				(right.condition_grade === null ? 0 : conditionRank(right.condition_grade) + 1);
			break;
		case 'title':
			comparison = compareText(left.title, right.title);
			break;
		case 'author':
			comparison = compareText(
				firstAuthors.get(left.issue_id) ?? '',
				firstAuthors.get(right.issue_id) ?? ''
			);
			break;
		case 'added': {
			const leftTime =
				left.created_at === null ? Number.NEGATIVE_INFINITY : Date.parse(left.created_at);
			const rightTime =
				right.created_at === null ? Number.NEGATIVE_INFINITY : Date.parse(right.created_at);
			comparison = leftTime === rightTime ? 0 : leftTime - rightTime;
			break;
		}
		default:
			comparison = left.issue_number - right.issue_number;
	}
	// Only the selected key changes direction. SQL keeps all secondary keys ascending.
	return (
		(direction === 'desc' ? -comparison : comparison) ||
		compareText(left.series_name, right.series_name) ||
		left.issue_number - right.issue_number ||
		(left.status === 'missing' ? left.issue_id : left.id) -
			(right.status === 'missing' ? right.issue_id : right.id)
	);
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
	const firstAuthors = new Map<number, string>();
	if (params.sort === 'author') {
		for (const entry of filtered) {
			if (firstAuthors.has(entry.issue_id)) continue;
			const authors = authorsByIssue.get(entry.issue_id) ?? [];
			firstAuthors.set(
				entry.issue_id,
				authors.reduce(
					(minimum, author) => (compareText(author, minimum) < 0 ? author : minimum),
					authors[0] ?? ''
				)
			);
		}
	}
	filtered = filtered.sort((left, right) =>
		compareEntries(left, right, params.sort ?? 'issue_number', direction, firstAuthors)
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
