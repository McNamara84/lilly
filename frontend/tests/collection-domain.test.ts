import { describe, expect, it } from 'vitest';
import {
	CONDITION_DEFINITIONS,
	CONDITION_GRADES,
	getConditionDefinition,
	isConditionGrade
} from '$lib/collection/conditions';
import {
	buildSeriesGridItems,
	COLLECTION_STATUS_PRESENTATION,
	effectiveCollectionEntry
} from '$lib/collection/series-grid';
import type { CollectionEntry } from '$lib/api/collection';
import type { Issue } from '$lib/api/series';
import {
	MAX_COLLECTION_NOTE_LENGTH,
	countCollectionNoteCharacters,
	limitCollectionNote
} from '$lib/collection/notes';

function issue(id: number, issueNumber: number): Issue {
	return {
		id,
		series_id: 1,
		issue_number: issueNumber,
		title: `Heft ${issueNumber}`,
		authors: [],
		published_at: null,
		part_number: null,
		part_total: null,
		cycle: null,
		cover_artists: [],
		keywords: [],
		notes: [],
		cover_url: null,
		cover_local_path: null,
		source_wiki_url: null
	};
}

function entry(id: number, issueId: number, status: CollectionEntry['status']): CollectionEntry {
	return {
		id,
		issue_id: issueId,
		issue_number: issueId,
		title: `Heft ${issueId}`,
		series_id: 1,
		series_name: 'Maddrax',
		series_slug: 'maddrax',
		cover_url: null,
		cover_local_path: null,
		copy_number: 1,
		condition_grade: 'Z2',
		status,
		notes: null,
		created_at: null,
		updated_at: null
	};
}

describe('condition scale', () => {
	it('defines exactly the authoritative Z0 through Z4 scale', () => {
		expect(CONDITION_GRADES).toEqual(['Z0', 'Z1', 'Z2', 'Z3', 'Z4']);
		expect(CONDITION_DEFINITIONS.map(({ value }) => value)).toEqual(CONDITION_GRADES);
		expect(CONDITION_DEFINITIONS).toHaveLength(5);
	});

	it('contains the distinguishing details from the collector definition', () => {
		expect(getConditionDefinition('Z0')?.description).toContain('Innenseiten noch weiß');
		expect(getConditionDefinition('Z1')?.description).toContain('ohne Risse');
		expect(getConditionDefinition('Z2')?.description).toContain('kleinen Rissen am Rand');
		expect(getConditionDefinition('Z3')?.description).toContain(
			'keine losen oder fehlenden Seiten'
		);
		expect(getConditionDefinition('Z4')?.description).toContain('lose oder fehlende Seiten');
	});

	it('accepts only exact condition values and handles absent definitions', () => {
		for (const grade of CONDITION_GRADES) expect(isConditionGrade(grade)).toBe(true);
		for (const invalid of ['Z5', 'z0', '', null, 2]) expect(isConditionGrade(invalid)).toBe(false);
		expect(getConditionDefinition(null)).toBeUndefined();
		expect(getConditionDefinition(undefined)).toBeUndefined();
	});
});

describe('collection notes', () => {
	it('counts Unicode code points consistently with the backend', () => {
		expect(countCollectionNoteCharacters('Grüße 📚')).toBe(7);
		expect(countCollectionNoteCharacters('📚'.repeat(MAX_COLLECTION_NOTE_LENGTH))).toBe(10_000);
	});

	it('limits oversized notes without splitting surrogate pairs', () => {
		const limited = limitCollectionNote('📚'.repeat(MAX_COLLECTION_NOTE_LENGTH + 1));

		expect(countCollectionNoteCharacters(limited)).toBe(MAX_COLLECTION_NOTE_LENGTH);
		expect(limited.endsWith('📚')).toBe(true);
	});
});

describe('series grid domain', () => {
	it('uses duplicate, owned, wanted, missing as deterministic priority', () => {
		const copies = [
			entry(4, 10, 'missing'),
			entry(3, 10, 'wanted'),
			entry(2, 10, 'owned'),
			entry(1, 10, 'duplicate')
		];

		expect(effectiveCollectionEntry(copies)?.id).toBe(1);
		expect(
			effectiveCollectionEntry(copies.filter(({ status }) => status !== 'duplicate'))?.id
		).toBe(2);
		expect(
			effectiveCollectionEntry(
				copies.filter(({ status }) => status !== 'duplicate' && status !== 'owned')
			)?.id
		).toBe(3);
		expect(effectiveCollectionEntry([copies[0]])).toBeNull();
	});

	it('uses the lower entry id to break equal-status ties without mutating input', () => {
		const copies = [entry(9, 10, 'owned'), entry(2, 10, 'owned')];

		expect(effectiveCollectionEntry(copies)?.id).toBe(2);
		expect(copies.map(({ id }) => id)).toEqual([9, 2]);
	});

	it('sorts issues numerically and maps all four visual states', () => {
		const issues = [issue(30, 3), issue(10, 1), issue(20, 2), issue(40, 4)];
		const entries = [entry(1, 10, 'owned'), entry(2, 20, 'duplicate'), entry(3, 30, 'wanted')];

		const result = buildSeriesGridItems(issues, entries);

		expect(result.map(({ issue: item }) => item.issue_number)).toEqual([1, 2, 3, 4]);
		expect(result.map(({ status }) => status)).toEqual(['owned', 'duplicate', 'wanted', 'missing']);
		expect(issues.map(({ issue_number }) => issue_number)).toEqual([3, 1, 2, 4]);
	});

	it('defines a distinct label and abbreviation for every state', () => {
		expect(COLLECTION_STATUS_PRESENTATION).toEqual({
			owned: { label: 'Vorhanden', abbreviation: 'V' },
			duplicate: { label: 'Doppelt/Tauschbar', abbreviation: 'D' },
			wanted: { label: 'Gesucht', abbreviation: 'G' },
			missing: { label: 'Fehlend', abbreviation: 'F' }
		});
	});
});
