import { describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import { userEvent } from '@testing-library/user-event';
import SeriesStatusGrid from '$lib/components/collection/SeriesStatusGrid.svelte';
import CollectionNote from '$lib/components/collection/CollectionNote.svelte';
import type { CollectionEntry } from '$lib/api/collection';
import type { Issue } from '$lib/api/series';

function issue(id: number, issueNumber: number, title = `Heft ${issueNumber}`): Issue {
	return {
		id,
		series_id: 1,
		issue_number: issueNumber,
		title,
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
		edition_label: null,
		condition_grade: 'Z2',
		status,
		notes: null,
		created_at: null,
		updated_at: null
	};
}

describe('SeriesStatusGrid', () => {
	it('renders all four states with text abbreviations and an accessible legend', () => {
		const issues = [issue(1, 1), issue(2, 2), issue(3, 3), issue(4, 4)];
		const entries = [entry(11, 1, 'owned'), entry(12, 2, 'duplicate'), entry(13, 3, 'wanted')];

		render(SeriesStatusGrid, { props: { issues, entries, onselect: vi.fn() } });

		expect(screen.getAllByTestId('series-status-cell').map((cell) => cell.dataset.status)).toEqual([
			'owned',
			'duplicate',
			'wanted',
			'missing'
		]);
		for (const label of ['Vorhanden', 'Doppelt/Tauschbar', 'Gesucht', 'Fehlend']) {
			expect(screen.getByText(label)).toBeInTheDocument();
		}
		expect(screen.getByLabelText('Legende der Sammlungszustände')).toBeInTheDocument();
	});

	it('describes status and action in every cell accessible name', () => {
		render(SeriesStatusGrid, {
			props: { issues: [issue(42, 42, 'Dunkle Zukunft')], entries: [], onselect: vi.fn() }
		});

		expect(
			screen.getByRole('button', {
				name: 'Heft #42: Dunkle Zukunft. Status: Fehlend. Details öffnen.'
			})
		).toBeInTheDocument();
	});

	it('passes the issue, effective entry and triggering button to selection', async () => {
		const selectedIssue = issue(42, 42, 'Dunkle Zukunft');
		const selectedEntry = entry(7, 42, 'owned');
		const onselect = vi.fn();
		render(SeriesStatusGrid, {
			props: { issues: [selectedIssue], entries: [selectedEntry], onselect }
		});
		const user = userEvent.setup();

		const button = screen.getByRole('button');
		await user.click(button);

		expect(onselect).toHaveBeenCalledWith(selectedIssue, selectedEntry, button);
	});

	it('passes null for a missing entry and supports Enter and Space activation', async () => {
		const selectedIssue = issue(42, 42);
		const onselect = vi.fn();
		render(SeriesStatusGrid, { props: { issues: [selectedIssue], entries: [], onselect } });
		const user = userEvent.setup();

		const button = screen.getByRole('button');
		button.focus();
		await user.keyboard('{Enter}');

		expect(onselect).toHaveBeenCalledWith(selectedIssue, null, button);
		await user.keyboard(' ');
		expect(onselect).toHaveBeenCalledTimes(2);
	});
});

describe('CollectionNote', () => {
	it('preserves line breaks and unicode for public display', () => {
		render(CollectionNote, { props: { note: 'Zeile 1\nGrüße 📚' } });

		const note = screen.getByTestId('collection-note');
		expect(note).toHaveTextContent('Zeile 1 Grüße 📚');
		expect(note).toHaveClass('whitespace-pre-wrap');
	});

	it('shows the default or custom empty state', () => {
		const defaultView = render(CollectionNote, { props: { note: null } });
		expect(screen.getByTestId('collection-note-empty')).toHaveTextContent(
			'Keine öffentliche Notiz.'
		);
		defaultView.unmount();

		render(CollectionNote, { props: { note: '', emptyText: 'Keine Notiz vorhanden.' } });
		expect(screen.getByTestId('collection-note-empty')).toHaveTextContent('Keine Notiz vorhanden.');
	});
});
