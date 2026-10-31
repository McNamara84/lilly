import type {
	AddCollectionEntryRequest,
	CollectionEntry,
	UpdateCollectionEntryRequest
} from '#lib/api/collection.js';
import {
	editOfflineState,
	getStoredIssue,
	listStoredSeries,
	type OfflineWriteState
} from './database';
import type {
	CollectionMutation,
	CollectionSyncResponse,
	CollectionSyncResult,
	CreateCollectionMutation,
	StoredConflict,
	UpdateCollectionMutation
} from './types';

const activeSyncs = new Map<number, Promise<CollectionSyncResponse | null>>();
const LEASE_MS = 60_000;

function changed(): void {
	if (typeof window !== 'undefined') window.dispatchEvent(new Event('lilly:offline-change'));
	if (typeof BroadcastChannel !== 'undefined') {
		const channel = new BroadcastChannel('lilly-offline');
		channel.postMessage({ type: 'changed' });
		channel.close();
	}
}

function id(): string {
	if (typeof crypto?.randomUUID === 'function') return crypto.randomUUID();
	return 'xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx'.replace(/[xy]/g, (character) => {
		const random = Math.floor(Math.random() * 16);
		return (character === 'x' ? random : (random & 0x3) | 0x8).toString(16);
	});
}

function entryId(mutation: CollectionMutation): number {
	return mutation.operation === 'create' ? mutation.temp_entry_id : mutation.entry_id;
}

function ordered(state: OfflineWriteState): CollectionMutation[] {
	return [...state.mutations.values()].sort(
		(a, b) => a.created_at.localeCompare(b.created_at) || a.mutation_id.localeCompare(b.mutation_id)
	);
}

function timestamp(state: OfflineWriteState, userId: number): string {
	const key = `queue-clock:${userId}`;
	const value = Math.max(Date.now(), Number(state.metadata.get(key)?.value ?? 0) + 1);
	state.metadata.set(key, { key, user_id: userId, value });
	return new Date(value).toISOString();
}

function updatedEntry(
	current: CollectionEntry,
	changes: UpdateCollectionEntryRequest
): CollectionEntry {
	return {
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
	const itemSeries = series.find((item) => item.id === issue.series_id);
	if (!itemSeries) throw new Error('Die Serie ist offline noch nicht verfügbar.');
	const result = await editOfflineState(userId, (state) => {
		const key = `temporary-id:${userId}`;
		const temporaryId = Math.min(-Date.now(), Number(state.metadata.get(key)?.value ?? 0) - 1);
		state.metadata.set(key, { key, user_id: userId, value: temporaryId });
		const now = timestamp(state, userId);
		const entry: CollectionEntry = {
			id: temporaryId,
			issue_id: issue.id,
			issue_number: issue.issue_number,
			title: issue.title,
			series_id: issue.series_id,
			series_name: itemSeries.name,
			series_slug: itemSeries.slug,
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
			mutation_id: id(),
			user_id: userId,
			operation: 'create',
			temp_entry_id: entry.id,
			entry: { ...request },
			created_at: now,
			attempts: 0,
			last_error: null
		};
		state.entries.set(entry.id, entry);
		state.mutations.set(mutation.mutation_id, mutation);
		return { entry, mutation };
	});
	changed();
	return result;
}

export async function queueCollectionUpdate(
	userId: number,
	targetId: number,
	changes: UpdateCollectionEntryRequest
): Promise<{ entry: CollectionEntry; mutation: CollectionMutation }> {
	const result = await editOfflineState(userId, (state) => {
		const current = state.entries.get(targetId);
		if (!current) throw new Error('Der Sammlungseintrag ist offline noch nicht verfügbar.');
		if (current.sync_state === 'conflict')
			throw new Error('Bitte löse zuerst den Offline-Konflikt dieses Heftes.');
		const pending = ordered(state).filter((item) => entryId(item) === targetId);
		const previous = pending.at(-1);
		if (targetId < 0 && !pending.some((item) => item.operation === 'create'))
			throw new Error('Die ausstehende Offline-Änderung wurde nicht gefunden.');
		let mutation: CollectionMutation;
		if (previous && !previous.sent && previous.attempts === 0) {
			mutation =
				previous.operation === 'create'
					? { ...previous, entry: { ...previous.entry, ...changes } }
					: { ...previous, changes: { ...previous.changes, ...changes } };
		} else {
			mutation = {
				mutation_id: id(),
				user_id: userId,
				operation: 'update',
				entry_id: targetId,
				base_revision: current.revision ?? 0,
				changes: { ...changes },
				created_at: timestamp(state, userId),
				attempts: 0,
				last_error: null,
				...(previous ? { depends_on: previous.mutation_id } : {})
			};
		}
		const entry = updatedEntry(current, changes);
		state.entries.set(targetId, entry);
		state.mutations.set(mutation.mutation_id, mutation);
		return { entry, mutation };
	});
	changed();
	return result;
}

function payload(mutation: CollectionMutation): Record<string, unknown> {
	return mutation.operation === 'create'
		? { operation: 'create', mutation_id: mutation.mutation_id, entry: mutation.entry }
		: {
				operation: 'update',
				mutation_id: mutation.mutation_id,
				entry_id: mutation.entry_id,
				base_revision: mutation.base_revision,
				changes: mutation.changes
			};
}

function applyResult(
	state: OfflineWriteState,
	mutation: CollectionMutation,
	result: CollectionSyncResult
): void {
	const localId = entryId(mutation);
	const local = state.entries.get(localId);
	const successors = ordered(state).filter(
		(item) =>
			item.operation === 'update' &&
			item.entry_id === localId &&
			item.mutation_id !== mutation.mutation_id
	) as UpdateCollectionMutation[];
	state.mutations.delete(mutation.mutation_id);
	if (result.status === 'applied' || result.status === 'already_applied') {
		if (mutation.operation === 'create') state.entries.delete(localId);
		if (result.entry) {
			for (const successor of successors) {
				successor.entry_id = result.entry.id;
				if (successor.depends_on === mutation.mutation_id) {
					delete successor.depends_on;
					successor.base_revision = result.entry.revision ?? 0;
				}
			}
			const entry =
				successors.length && local
					? {
							...local,
							id: result.entry.id,
							revision: result.entry.revision,
							sync_state: 'pending' as const
						}
					: { ...result.entry, sync_state: 'synced' as const };
			state.entries.set(entry.id, entry);
		}
		return;
	}
	// Park all subsequent edits with this conflict; the explicit decision applies to the latest intent.
	let conflictMutation = mutation;
	for (const successor of successors) {
		conflictMutation =
			conflictMutation.operation === 'create'
				? { ...conflictMutation, entry: { ...conflictMutation.entry, ...successor.changes } }
				: { ...conflictMutation, changes: { ...conflictMutation.changes, ...successor.changes } };
		state.mutations.delete(successor.mutation_id);
	}
	state.conflicts.set(mutation.mutation_id, {
		mutation_id: mutation.mutation_id,
		user_id: mutation.user_id,
		created_at: new Date().toISOString(),
		mutation: conflictMutation,
		server_entry: result.entry,
		error: result.error ?? 'Die Änderung konnte nicht synchronisiert werden.',
		code: result.code,
		status: result.status
	});
	if (local) state.entries.set(localId, { ...local, sync_state: 'conflict' });
}

export async function synchronizeCollection(
	userId: number,
	send: (mutations: Record<string, unknown>[]) => Promise<CollectionSyncResponse>
): Promise<CollectionSyncResponse | null> {
	const running = activeSyncs.get(userId);
	if (running) {
		const earlier = await running;
		// A caller may have queued new work after the running drain selected its final batch.
		const later = await synchronizeCollection(userId, send);
		return later ? { results: [...(earlier?.results ?? []), ...later.results] } : earlier;
	}
	const promise = drainQueue(userId, send);
	activeSyncs.set(userId, promise);
	try {
		return await promise;
	} finally {
		activeSyncs.delete(userId);
	}
}

async function drainQueue(
	userId: number,
	send: (mutations: Record<string, unknown>[]) => Promise<CollectionSyncResponse>
): Promise<CollectionSyncResponse | null> {
	const key = `sync-lease:${userId}`;
	const owner = id();
	const owns = (state: OfflineWriteState) => {
		const lease = state.metadata.get(key);
		return lease ? (JSON.parse(String(lease.value)) as { owner: string }).owner === owner : false;
	};
	const renew = (state: OfflineWriteState) =>
		state.metadata.set(key, {
			key,
			user_id: userId,
			value: JSON.stringify({ owner, expires: Date.now() + LEASE_MS })
		});
	const claimed = await editOfflineState(userId, (state) => {
		if (!state.mutations.size) return false;
		const existing = state.metadata.get(key);
		if (
			existing &&
			(JSON.parse(String(existing.value)) as { expires: number }).expires > Date.now()
		)
			return false;
		renew(state);
		return true;
	});
	if (!claimed) return null;
	const heartbeat = setInterval(() => {
		void editOfflineState(userId, (state) => {
			if (owns(state)) renew(state);
		}).catch(() => undefined);
	}, LEASE_MS / 3);
	const results: CollectionSyncResult[] = [];
	try {
		while (true) {
			const batch = await editOfflineState(userId, (state) => {
				if (!owns(state)) return [];
				renew(state);
				const next = ordered(state)
					.filter((item) => !item.depends_on)
					.slice(0, 100);
				for (const mutation of next) {
					mutation.sent = true;
					mutation.attempts++;
				}
				return next;
			});
			if (!batch.length) break;
			try {
				const response = await send(batch.map(payload));
				const committed = await editOfflineState(userId, (state) => {
					if (!owns(state)) return false;
					for (const result of response.results) {
						const mutation = state.mutations.get(result.mutation_id);
						if (mutation && batch.some((item) => item.mutation_id === result.mutation_id))
							applyResult(state, mutation, result);
					}
					return true;
				});
				if (!committed) break;
				results.push(...response.results);
				if (
					batch.some(
						(item) => !response.results.some((result) => result.mutation_id === item.mutation_id)
					)
				)
					throw new Error('Die Synchronisierung hat nicht alle Änderungen bestätigt.');
				changed();
			} catch (error) {
				await editOfflineState(userId, (state) => {
					if (!owns(state)) return;
					for (const item of batch) {
						const current = state.mutations.get(item.mutation_id);
						if (current)
							current.last_error =
								error instanceof Error ? error.message : 'Synchronisierung fehlgeschlagen';
					}
				}).catch(() => undefined);
				throw error;
			}
		}
		return { results };
	} finally {
		clearInterval(heartbeat);
		await editOfflineState(userId, (state) => {
			if (owns(state)) state.metadata.delete(key);
		}).catch(() => undefined);
		changed();
	}
}

export async function acceptConflictServerVersion(conflict: StoredConflict): Promise<void> {
	await editOfflineState(conflict.user_id, (state) => {
		const stored = state.conflicts.get(conflict.mutation_id);
		if (!stored) return;
		conflict = stored;
		state.entries.delete(entryId(conflict.mutation));
		if (conflict.server_entry)
			state.entries.set(conflict.server_entry.id, {
				...conflict.server_entry,
				sync_state: 'synced'
			});
		state.conflicts.delete(conflict.mutation_id);
	});
	changed();
}

export async function reapplyConflictLocalVersion(
	conflict: StoredConflict
): Promise<CollectionMutation> {
	const mutation = await editOfflineState(conflict.user_id, (state) => {
		// Read the durable decision inside the transaction, rather than a stale UI proxy.
		const stored = state.conflicts.get(conflict.mutation_id);
		if (!stored) throw new Error('Der Konflikt wurde bereits gelöst.');
		if (stored.mutation.operation !== 'update' || !stored.server_entry?.revision)
			throw new Error('Diese Änderung kann nicht erneut angewendet werden.');
		conflict = stored;
		const mutation: UpdateCollectionMutation = {
			...stored.mutation,
			mutation_id: id(),
			base_revision: stored.server_entry.revision,
			sent: false,
			attempts: 0,
			last_error: null,
			created_at: timestamp(state, conflict.user_id)
		};
		delete mutation.depends_on;
		state.mutations.set(mutation.mutation_id, mutation);
		state.entries.set(mutation.entry_id, updatedEntry(conflict.server_entry!, mutation.changes));
		state.conflicts.delete(conflict.mutation_id);
		return mutation;
	});
	changed();
	return mutation;
}
