import type { MeResponse } from '$lib/api/auth';
import type { CollectionEntry } from '$lib/api/collection';
import type { Issue, Series } from '$lib/api/series';
import type { OfflineSnapshot } from '$lib/offline/types';

export const profile = (id: number): MeResponse => ({
	id,
	email: `user-${id}@example.test`,
	display_name: `User ${id}`,
	email_verified: true,
	role: 'user'
});

export const series: Series = {
	id: 10,
	name: 'Maddrax',
	slug: 'maddrax',
	publisher: null,
	genre: null,
	frequency: null,
	total_issues: 1,
	status: 'running',
	active: true,
	source_url: null
};

export const issue: Issue = {
	id: 20,
	series_id: 10,
	issue_number: 1,
	title: 'Der Gott aus dem Eis',
	authors: ['Jo Zybell'],
	published_at: null,
	part_number: null,
	part_total: null,
	cycle: null,
	cover_artists: [],
	keywords: [],
	notes: [],
	cover_url: null,
	cover_local_path: '/media/covers/series-10/1.png',
	source_wiki_url: null
};

export const entry: CollectionEntry = {
	id: 30,
	issue_id: 20,
	issue_number: 1,
	title: issue.title,
	series_id: 10,
	series_name: series.name,
	series_slug: series.slug,
	cover_url: null,
	cover_local_path: issue.cover_local_path,
	copy_number: 1,
	edition_label: null,
	condition_grade: 'Z2',
	status: 'owned',
	notes: null,
	revision: 1,
	created_at: '2026-08-15T01:00:00Z',
	updated_at: '2026-08-15T01:00:00Z'
};

export function snapshot(userId: number, entries: CollectionEntry[] = [entry]): OfflineSnapshot {
	return {
		schema_version: 1,
		snapshot_version: `version-${userId}`,
		user_id: userId,
		generated_at: '2026-08-15T01:02:03Z',
		series: [series],
		issues: [issue],
		collection_entries: entries
	};
}
