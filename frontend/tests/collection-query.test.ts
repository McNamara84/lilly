import { describe, expect, it } from 'vitest';
import {
	collectionQueryHref,
	countActiveCollectionFilters,
	normalizeCollectionQuery,
	parseCollectionQuery,
	serializeCollectionQuery
} from '../src/lib/utils/collection-query';

describe('collection query utilities', () => {
	it('parses every supported collection parameter', () => {
		const result = parseCollectionQuery(
			new URLSearchParams(
				'series_slug=maddrax&status=owned&issue_number=42&condition=Z2&title=dunkle&author=zybell&sort=author&sort_dir=desc&page=3&per_page=25'
			)
		);

		expect(result).toEqual({
			series_slug: 'maddrax',
			status: 'owned',
			issue_number: 42,
			condition: 'Z2',
			title: 'dunkle',
			author: 'zybell',
			sort: 'author',
			sort_dir: 'desc',
			page: 3,
			per_page: 25
		});
	});

	it('removes invalid values and canonical defaults', () => {
		const result = parseCollectionQuery(
			new URLSearchParams(
				'status=sold&issue_number=-1&condition=Z9&sort=price&sort_dir=up&page=1&per_page=500'
			)
		);

		expect(result).toEqual({ per_page: 100 });
	});

	it('trims text values and omits default sorting when serializing', () => {
		const query = serializeCollectionQuery({
			series_slug: ' maddrax ',
			title: ' Zukunft ',
			author: ' Zybell ',
			sort: 'issue_number',
			sort_dir: 'asc',
			page: 1
		});

		expect(query.toString()).toBe('series_slug=maddrax&title=Zukunft&author=Zybell');
	});

	it('requires a series for missing issues and removes their condition', () => {
		expect(normalizeCollectionQuery({ status: 'missing', condition: 'Z1' })).toEqual({});
		expect(
			normalizeCollectionQuery({
				series_slug: 'maddrax',
				status: 'missing',
				condition: 'Z1'
			})
		).toEqual({ series_slug: 'maddrax', status: 'missing' });
		expect(
			normalizeCollectionQuery({
				series_slug: 'maddrax',
				status: 'missing',
				sort: 'added'
			})
		).toEqual({ series_slug: 'maddrax', status: 'missing' });
	});

	it('creates canonical collection links', () => {
		expect(collectionQueryHref({})).toBe('/collection');
		expect(collectionQueryHref({ title: 'Dunkle Zukunft', sort_dir: 'desc' })).toBe(
			'/collection?title=Dunkle+Zukunft&sort_dir=desc'
		);
	});

	it('counts only user-facing filters', () => {
		expect(
			countActiveCollectionFilters({
				series_slug: 'maddrax',
				status: 'owned',
				title: 'Dunkle',
				sort: 'author',
				sort_dir: 'desc'
			})
		).toBe(3);
	});
});
