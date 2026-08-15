import type {
	AddCollectionEntryRequest,
	CollectionEntry,
	UpdateCollectionEntryRequest
} from '$lib/api/collection';
import type { MeResponse } from '$lib/api/auth';
import type { Issue, Series } from '$lib/api/series';

export interface OfflineSnapshot {
	schema_version: number;
	snapshot_version: string;
	user_id: number;
	generated_at: string;
	series: Series[];
	issues: Issue[];
	collection_entries: CollectionEntry[];
}

export interface StoredProfile {
	user_id: number;
	profile: MeResponse;
	confirmed_at: string;
}

export interface OfflineMetadata {
	key: string;
	user_id: number;
	value: string | number;
}

export interface StoredSeries extends Series {
	key: string;
	user_id: number;
}

export interface StoredIssue extends Issue {
	key: string;
	user_id: number;
}

export interface StoredCollectionEntry extends CollectionEntry {
	key: string;
	user_id: number;
}

interface MutationBase {
	mutation_id: string;
	user_id: number;
	created_at: string;
	attempts: number;
	last_error: string | null;
}

export interface CreateCollectionMutation extends MutationBase {
	operation: 'create';
	temp_entry_id: number;
	entry: AddCollectionEntryRequest;
}

export interface UpdateCollectionMutation extends MutationBase {
	operation: 'update';
	entry_id: number;
	base_revision: number;
	changes: UpdateCollectionEntryRequest;
}

export type CollectionMutation = CreateCollectionMutation | UpdateCollectionMutation;

export type CollectionSyncStatus = 'applied' | 'already_applied' | 'conflict' | 'rejected';

export interface CollectionSyncResult {
	mutation_id: string;
	status: CollectionSyncStatus;
	entry: CollectionEntry | null;
	error: string | null;
	code: string | null;
}

export interface CollectionSyncResponse {
	results: CollectionSyncResult[];
}

export interface StoredConflict {
	mutation_id: string;
	user_id: number;
	created_at: string;
	mutation: CollectionMutation;
	server_entry: CollectionEntry | null;
	error: string;
	code: string | null;
	status: 'conflict' | 'rejected';
}
