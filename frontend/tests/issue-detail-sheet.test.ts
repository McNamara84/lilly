import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/svelte';
import { userEvent } from '@testing-library/user-event';
import IssueDetailSheet from '../src/lib/components/collection/IssueDetailSheet.svelte';
import type { Issue } from '$lib/api/series';
import type { CollectionEntry } from '$lib/api/collection';

const sampleIssue: Issue = {
	id: 42,
	series_id: 1,
	issue_number: 42,
	title: 'Dunkle Zukunft',
	cover_url: 'https://example.com/cover.jpg',
	cover_local_path: null,
	source_wiki_url: 'https://wiki.example.com/42',
	authors: ['Jo Zybell'],
	cycle: 'Kreuzfahrt',
	published_at: '2000-02-11',
	keywords: ['Zukunft'],
	cover_artists: [],
	notes: []
};

const sampleEntry: CollectionEntry = {
	id: 1,
	issue_id: 42,
	issue_number: 42,
	title: 'Dunkle Zukunft',
	series_id: 1,
	series_name: 'Maddrax',
	series_slug: 'maddrax',
	cover_url: null,
	cover_local_path: null,
	copy_number: 1,
	condition_grade: 'Z3',
	status: 'duplicate',
	notes: 'My note',
	created_at: '2026-03-22T10:00:00Z',
	updated_at: '2026-03-22T10:00:00Z'
};

describe('IssueDetailSheet', () => {
	let onclose: () => void;
	let onsave: (data: {
		issue_id: number;
		condition_grade: string;
		status?: string;
		notes?: string;
	}) => void;
	let ondelete: () => void;

	beforeEach(() => {
		onclose = vi.fn();
		onsave = vi.fn();
		ondelete = vi.fn();
	});

	it('does not render when issue is null', () => {
		render(IssueDetailSheet, {
			props: { issue: null, collection_entry: null, onclose, onsave }
		});

		expect(screen.queryByTestId('issue-detail-sheet')).not.toBeInTheDocument();
	});

	it('renders sheet when issue is provided', () => {
		render(IssueDetailSheet, {
			props: { issue: sampleIssue, collection_entry: null, onclose, onsave }
		});

		expect(screen.getByTestId('issue-detail-sheet')).toBeInTheDocument();
		expect(screen.getByText('Dunkle Zukunft')).toBeInTheDocument();
		expect(screen.getByText('#42')).toBeInTheDocument();
	});

	it('renders backdrop that closes sheet on click', async () => {
		render(IssueDetailSheet, {
			props: { issue: sampleIssue, collection_entry: null, onclose, onsave }
		});
		const user = userEvent.setup();

		await user.click(screen.getByTestId('detail-sheet-backdrop'));

		expect(onclose).toHaveBeenCalledOnce();
	});

	it('closes sheet on Escape key', async () => {
		render(IssueDetailSheet, {
			props: { issue: sampleIssue, collection_entry: null, onclose, onsave }
		});

		await fireEvent.keyDown(screen.getByTestId('detail-sheet-backdrop'), { key: 'Escape' });

		expect(onclose).toHaveBeenCalled();
	});

	it('shows "Hinzufügen" button when no collection entry', () => {
		render(IssueDetailSheet, {
			props: { issue: sampleIssue, collection_entry: null, onclose, onsave }
		});

		expect(screen.getByTestId('save-button')).toHaveTextContent('Hinzufügen');
	});

	it('shows "Speichern" button when editing existing entry', () => {
		render(IssueDetailSheet, {
			props: { issue: sampleIssue, collection_entry: sampleEntry, onclose, onsave }
		});

		expect(screen.getByTestId('save-button')).toHaveTextContent('Speichern');
	});

	it('shows delete button when editing with ondelete', () => {
		render(IssueDetailSheet, {
			props: { issue: sampleIssue, collection_entry: sampleEntry, onclose, onsave, ondelete }
		});

		expect(screen.getByTestId('delete-button')).toBeInTheDocument();
		expect(screen.getByTestId('delete-button')).toHaveTextContent('Entfernen');
	});

	it('hides delete button when adding', () => {
		render(IssueDetailSheet, {
			props: { issue: sampleIssue, collection_entry: null, onclose, onsave, ondelete }
		});

		expect(screen.queryByTestId('delete-button')).not.toBeInTheDocument();
	});

	it('calls ondelete when delete button is clicked', async () => {
		render(IssueDetailSheet, {
			props: { issue: sampleIssue, collection_entry: sampleEntry, onclose, onsave, ondelete }
		});
		const user = userEvent.setup();

		await user.click(screen.getByTestId('delete-button'));

		expect(ondelete).toHaveBeenCalledOnce();
	});

	it('calls onsave with correct data when save button is clicked', async () => {
		render(IssueDetailSheet, {
			props: { issue: sampleIssue, collection_entry: null, onclose, onsave }
		});
		const user = userEvent.setup();

		await user.click(screen.getByTestId('save-button'));

		expect(onsave).toHaveBeenCalledWith({
			issue_id: 42,
			condition_grade: 'Z2',
			status: 'owned',
			notes: undefined
		});
	});

	it('pre-fills fields from existing collection entry', () => {
		render(IssueDetailSheet, {
			props: { issue: sampleIssue, collection_entry: sampleEntry, onclose, onsave }
		});

		const notesTextarea = screen.getByTestId('notes-textarea') as HTMLTextAreaElement;
		expect(notesTextarea.value).toBe('My note');
	});

	it('renders status radio buttons', () => {
		render(IssueDetailSheet, {
			props: { issue: sampleIssue, collection_entry: null, onclose, onsave }
		});

		expect(screen.getByTestId('status-owned')).toBeInTheDocument();
		expect(screen.getByTestId('status-duplicate')).toBeInTheDocument();
		expect(screen.getByTestId('status-wanted')).toBeInTheDocument();
	});

	it('status buttons toggle correctly', async () => {
		render(IssueDetailSheet, {
			props: { issue: sampleIssue, collection_entry: null, onclose, onsave }
		});
		const user = userEvent.setup();

		// Default: owned is active
		expect(screen.getByTestId('status-owned')).toHaveAttribute('aria-checked', 'true');
		expect(screen.getByTestId('status-wanted')).toHaveAttribute('aria-checked', 'false');

		// Click wanted
		await user.click(screen.getByTestId('status-wanted'));

		expect(screen.getByTestId('status-wanted')).toHaveAttribute('aria-checked', 'true');
		expect(screen.getByTestId('status-owned')).toHaveAttribute('aria-checked', 'false');
	});

	it('sends updated status when saving after toggle', async () => {
		render(IssueDetailSheet, {
			props: { issue: sampleIssue, collection_entry: null, onclose, onsave }
		});
		const user = userEvent.setup();

		await user.click(screen.getByTestId('status-wanted'));
		await user.click(screen.getByTestId('save-button'));

		expect(onsave).toHaveBeenCalledWith(
			expect.objectContaining({
				status: 'wanted'
			})
		);
	});

	it('shows cover image when available', () => {
		render(IssueDetailSheet, {
			props: { issue: sampleIssue, collection_entry: null, onclose, onsave }
		});

		const img = screen.getByAltText('Cover von #42: Dunkle Zukunft');
		expect(img).toBeInTheDocument();
		expect(img.getAttribute('src')).toContain('cover.jpg');
	});

	it('shows placeholder when no cover', () => {
		render(IssueDetailSheet, {
			props: {
				issue: { ...sampleIssue, cover_url: null, cover_local_path: null },
				collection_entry: null,
				onclose,
				onsave
			}
		});

		expect(screen.queryByAltText('Cover von #42: Dunkle Zukunft')).not.toBeInTheDocument();
		expect(screen.getAllByText('#42').length).toBeGreaterThanOrEqual(1);
	});

	it('shows author and cycle info', () => {
		render(IssueDetailSheet, {
			props: { issue: sampleIssue, collection_entry: null, onclose, onsave }
		});

		expect(screen.getByText('Jo Zybell')).toBeInTheDocument();
		expect(screen.getByText(/Kreuzfahrt/)).toBeInTheDocument();
	});

	it('shows wiki link when available', () => {
		render(IssueDetailSheet, {
			props: { issue: sampleIssue, collection_entry: null, onclose, onsave }
		});

		const link = screen.getByTestId('wiki-link');
		expect(link).toHaveAttribute('href', 'https://wiki.example.com/42');
		expect(link).toHaveAttribute('target', '_blank');
	});

	it('hides wiki link when not available', () => {
		render(IssueDetailSheet, {
			props: {
				issue: { ...sampleIssue, source_wiki_url: null },
				collection_entry: null,
				onclose,
				onsave
			}
		});

		expect(screen.queryByTestId('wiki-link')).not.toBeInTheDocument();
	});

	it('has dialog role and aria-modal', () => {
		render(IssueDetailSheet, {
			props: { issue: sampleIssue, collection_entry: null, onclose, onsave }
		});

		const dialog = screen.getByRole('dialog');
		expect(dialog).toHaveAttribute('aria-modal', 'true');
		expect(dialog).toHaveAttribute('aria-label', 'Heftdetails: Dunkle Zukunft');
	});

	it('renders notes textarea with placeholder', () => {
		render(IssueDetailSheet, {
			props: { issue: sampleIssue, collection_entry: null, onclose, onsave }
		});

		const textarea = screen.getByTestId('notes-textarea');
		expect(textarea).toHaveAttribute('placeholder', 'Optionale Notizen...');
	});

	it('includes notes when saving with text', async () => {
		render(IssueDetailSheet, {
			props: { issue: sampleIssue, collection_entry: null, onclose, onsave }
		});
		const user = userEvent.setup();

		const textarea = screen.getByTestId('notes-textarea');
		await user.type(textarea, 'Test notes');
		await user.click(screen.getByTestId('save-button'));

		expect(onsave).toHaveBeenCalledWith(
			expect.objectContaining({
				notes: 'Test notes'
			})
		);
	});

	it('prefers cover_local_path over cover_url', () => {
		render(IssueDetailSheet, {
			props: {
				issue: {
					...sampleIssue,
					cover_local_path: '/covers/local.jpg',
					cover_url: 'https://example.com/remote.jpg'
				},
				collection_entry: null,
				onclose,
				onsave
			}
		});

		const img = screen.getByAltText('Cover von #42: Dunkle Zukunft') as HTMLImageElement;
		expect(img.src).toContain('local.jpg');
	});

	it('hides authors section when empty', () => {
		render(IssueDetailSheet, {
			props: {
				issue: { ...sampleIssue, authors: [] },
				collection_entry: null,
				onclose,
				onsave
			}
		});

		expect(screen.queryByText('Jo Zybell')).not.toBeInTheDocument();
	});

	it('hides cycle section when null', () => {
		render(IssueDetailSheet, {
			props: {
				issue: { ...sampleIssue, cycle: null },
				collection_entry: null,
				onclose,
				onsave
			}
		});

		expect(screen.queryByText(/Zyklus/)).not.toBeInTheDocument();
	});
});
