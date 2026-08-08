import type { CollectionEntry, CollectionStatus } from '$lib/api/collection';
import type { Issue } from '$lib/api/series';

export interface SeriesGridItem {
	issue: Issue;
	status: CollectionStatus;
	entry: CollectionEntry | null;
}

const STATUS_PRIORITY: Record<CollectionStatus, number> = {
	duplicate: 3,
	owned: 2,
	wanted: 1,
	missing: 0
};

export function effectiveCollectionEntry(
	entries: readonly CollectionEntry[]
): CollectionEntry | null {
	return (
		entries
			.filter((entry) => entry.status !== 'missing')
			.sort(
				(left, right) =>
					STATUS_PRIORITY[right.status] - STATUS_PRIORITY[left.status] || left.id - right.id
			)[0] ?? null
	);
}

export function buildSeriesGridItems(
	issues: readonly Issue[],
	entries: readonly CollectionEntry[]
): SeriesGridItem[] {
	const byIssue = new Map<number, CollectionEntry[]>();
	for (const entry of entries) {
		const existing = byIssue.get(entry.issue_id);
		if (existing) existing.push(entry);
		else byIssue.set(entry.issue_id, [entry]);
	}

	return [...issues]
		.sort((left, right) => left.issue_number - right.issue_number || left.id - right.id)
		.map((issue) => {
			const entry = effectiveCollectionEntry(byIssue.get(issue.id) ?? []);
			return {
				issue,
				entry,
				status: entry?.status ?? 'missing'
			};
		});
}

export const COLLECTION_STATUS_PRESENTATION = {
	owned: { label: 'Vorhanden', abbreviation: 'V' },
	duplicate: { label: 'Doppelt/Tauschbar', abbreviation: 'D' },
	wanted: { label: 'Gesucht', abbreviation: 'G' },
	missing: { label: 'Fehlend', abbreviation: 'F' }
} as const satisfies Record<CollectionStatus, { label: string; abbreviation: string }>;
