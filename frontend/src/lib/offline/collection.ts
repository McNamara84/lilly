import type {
	AddCollectionEntryRequest,
	CollectionEntry,
	CollectionQueryParams,
	PaginatedCollection,
	UpdateCollectionEntryRequest
} from '$lib/api/collection';
import {
	deleteConflict,
	deleteMutation,
	deleteStoredCollectionEntry,
	enqueueMutation,
	getStoredIssue,
	listConflicts,
	listMutations,
	listStoredCollectionEntries,
	listStoredIssues,
	listStoredSeries,
	putCollectionEntry,
	putConflict
} from './database';
import type {
	CollectionMutation,
	CollectionSyncResponse,
	CreateCollectionMutation,
	StoredConflict,
	UpdateCollectionMutation
} from './types';

let temporaryId = -1;
const activeSyncs = new Map<number, Promise<CollectionSyncResponse | null>>();

function notifyOfflineStateChanged(): void {
	if (typeof window !== 'undefined') window.dispatchEvent(new Event('lilly:offline-change'));
}

function createMutationId(): string {
	if (typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function') {
		return crypto.randomUUID();
	}
	return 'xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx'.replace(/[xy]/g, (character) => {
		const random = Math.floor(Math.random() * 16);
		const value = character === 'x' ? random : (random & 0x3) | 0x8;
		return value.toString(16);
	});
}

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

export async function queueCollectionCreate(
	userId: number,
	request: AddCollectionEntryRequest
): Promise<{ entry: CollectionEntry; mutation: CreateCollectionMutation }> {
	const [issue, series] = await Promise.all([
		getStoredIssue(userId, request.issue_id),
		listStoredSeries(userId)
	]);
	if (!issue) throw new Error('Dieses Heft ist offline noch nicht verfügbar.');
	const issueSeries = series.find((item) => item.id === issue.series_id);
	if (!issueSeries) throw new Error('Die Serie ist offline noch nicht verfügbar.');

	const now = new Date().toISOString();
	const entry: CollectionEntry = {
		id: temporaryId--,
		issue_id: issue.id,
		issue_number: issue.issue_number,
		title: issue.title,
		series_id: issue.series_id,
		series_name: issueSeries.name,
		series_slug: issueSeries.slug,
		cover_url: issue.cover_url,
		cover_local_path: issue.cover_local_path,
		copy_number: request.copy_number ?? null,
		edition_label: request.edition_label?.trim() || null,
		condition_grade: request.condition_grade ?? null,
		status: request.status ?? 'owned',
		notes: request.notes?.trim() || null,
		revision: null,
		created_at: now,
		updated_at: now,
		sync_state: 'pending'
	};
	const mutation: CreateCollectionMutation = {
		mutation_id: createMutationId(),
		user_id: userId,
		operation: 'create',
		temp_entry_id: entry.id,
		entry: request,
		created_at: now,
		attempts: 0,
		last_error: null
	};
	await enqueueMutation(mutation);
	await putCollectionEntry(userId, entry);
	notifyOfflineStateChanged();
	return { entry, mutation };
}

export async function queueCollectionUpdate(
	userId: number,
	entryId: number,
	changes: UpdateCollectionEntryRequest
): Promise<{ entry: CollectionEntry; mutation: CollectionMutation }> {
	const entries = await listStoredCollectionEntries(userId);
	const current = entries.find((entry) => entry.id === entryId);
	if (!current) throw new Error('Der Sammlungseintrag ist offline noch nicht verfügbar.');
	const mutations = await listMutations(userId);

	let mutation: CollectionMutation;
	if (entryId < 0) {
		const pendingCreate = mutations.find(
			(item): item is CreateCollectionMutation =>
				item.operation === 'create' && item.temp_entry_id === entryId
		);
		if (!pendingCreate) throw new Error('Die ausstehende Offline-Änderung wurde nicht gefunden.');
		mutation = {
			...pendingCreate,
			entry: { ...pendingCreate.entry, ...changes }
		};
	} else {
		const pendingUpdate = mutations.find(
			(item): item is UpdateCollectionMutation =>
				item.operation === 'update' && item.entry_id === entryId
		);
		mutation = pendingUpdate
			? { ...pendingUpdate, changes: { ...pendingUpdate.changes, ...changes } }
			: {
					mutation_id: createMutationId(),
					user_id: userId,
					operation: 'update',
					entry_id: entryId,
					base_revision: current.revision ?? 0,
					changes,
					created_at: new Date().toISOString(),
					attempts: 0,
					last_error: null
				};
	}

	const updated: CollectionEntry = {
		...current,
		...changes,
		edition_label:
			changes.edition_label === undefined
				? current.edition_label
				: changes.edition_label.trim() || null,
		notes: changes.notes === undefined ? current.notes : changes.notes.trim() || null,
		updated_at: new Date().toISOString(),
		sync_state: 'pending'
	};
	await enqueueMutation(mutation);
	await putCollectionEntry(userId, updated);
	notifyOfflineStateChanged();
	return { entry: updated, mutation };
}

function toSyncPayload(mutation: CollectionMutation): Record<string, unknown> {
	if (mutation.operation === 'create') {
		return {
			operation: mutation.operation,
			mutation_id: mutation.mutation_id,
			entry: mutation.entry
		};
	}
	return {
		operation: mutation.operation,
		mutation_id: mutation.mutation_id,
		entry_id: mutation.entry_id,
		base_revision: mutation.base_revision,
		changes: mutation.changes
	};
}

export async function synchronizeCollection(
	userId: number,
	send: (mutations: Record<string, unknown>[]) => Promise<CollectionSyncResponse>
): Promise<CollectionSyncResponse | null> {
	const running = activeSyncs.get(userId);
	if (running) return running;

	const promise = (async () => {
		const mutations = await listMutations(userId);
		if (mutations.length === 0) return null;
		let response: CollectionSyncResponse;
		try {
			response = await send(mutations.map(toSyncPayload));
		} catch (error) {
			const message = error instanceof Error ? error.message : 'Synchronisierung fehlgeschlagen';
			for (const mutation of mutations) {
				await enqueueMutation({
					...mutation,
					attempts: mutation.attempts + 1,
					last_error: message
				});
			}
			throw error;
		}

		const mutationById = new Map(mutations.map((mutation) => [mutation.mutation_id, mutation]));
		for (const result of response.results) {
			const mutation = mutationById.get(result.mutation_id);
			if (!mutation) continue;
			if (result.status === 'applied' || result.status === 'already_applied') {
				await deleteMutation(mutation.mutation_id);
				if (mutation.operation === 'create') {
					await deleteStoredCollectionEntry(userId, mutation.temp_entry_id);
				}
				if (result.entry)
					await putCollectionEntry(userId, { ...result.entry, sync_state: 'synced' });
				continue;
			}

			const conflict: StoredConflict = {
				mutation_id: mutation.mutation_id,
				user_id: userId,
				created_at: new Date().toISOString(),
				mutation,
				server_entry: result.entry,
				error: result.error ?? 'Die Änderung konnte nicht synchronisiert werden.',
				code: result.code,
				status: result.status
			};
			await putConflict(conflict);
			await deleteMutation(mutation.mutation_id);
			const localEntryId =
				mutation.operation === 'create' ? mutation.temp_entry_id : mutation.entry_id;
			const localEntry = (await listStoredCollectionEntries(userId)).find(
				(entry) => entry.id === localEntryId
			);
			if (localEntry) {
				await putCollectionEntry(userId, { ...localEntry, sync_state: 'conflict' });
			}
		}
		notifyOfflineStateChanged();
		return response;
	})();

	activeSyncs.set(userId, promise);
	try {
		return await promise;
	} finally {
		activeSyncs.delete(userId);
	}
}

export async function acceptConflictServerVersion(conflict: StoredConflict): Promise<void> {
	const localEntryId =
		conflict.mutation.operation === 'create'
			? conflict.mutation.temp_entry_id
			: conflict.mutation.entry_id;
	if (conflict.server_entry) {
		await putCollectionEntry(conflict.user_id, {
			...conflict.server_entry,
			sync_state: 'synced'
		});
	}
	if (localEntryId !== conflict.server_entry?.id) {
		await deleteStoredCollectionEntry(conflict.user_id, localEntryId);
	}
	await deleteConflict(conflict.mutation_id);
	notifyOfflineStateChanged();
}

export async function reapplyConflictLocalVersion(
	conflict: StoredConflict
): Promise<CollectionMutation> {
	if (conflict.mutation.operation !== 'update' || !conflict.server_entry?.revision) {
		throw new Error('Diese Änderung kann nicht erneut angewendet werden.');
	}
	const mutation: UpdateCollectionMutation = {
		...conflict.mutation,
		mutation_id: createMutationId(),
		base_revision: conflict.server_entry.revision,
		created_at: new Date().toISOString(),
		attempts: 0,
		last_error: null
	};
	await enqueueMutation(mutation);
	await deleteConflict(conflict.mutation_id);
	notifyOfflineStateChanged();
	return mutation;
}

export { listConflicts, listMutations };
