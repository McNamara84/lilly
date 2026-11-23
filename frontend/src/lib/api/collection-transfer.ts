import { fetchMe } from './auth';
import { refreshOfflineSnapshot, syncPendingCollectionChanges } from './collection';
import { listConflicts, listMutations } from '#lib/offline/database.js';
import { offlineSessionVersion, isCurrentOfflineSession } from '#lib/offline/session.js';

export type TransferFormat = 'csv' | 'json';
export type TransferStatus =
	'prepared' | 'queued' | 'running' | 'completed' | 'ready' | 'failed' | 'cancelled' | 'expired';
export interface TransferRow {
	row_number: number;
	copy_key: string | null;
	status: 'new' | 'created' | 'unchanged' | 'conflict' | 'invalid' | 'failed';
	code: string | null;
	entry_id: number | null;
}
export interface TransferJob {
	id: string;
	kind: 'import' | 'export';
	format: TransferFormat | 'zip';
	status: TransferStatus;
	created_at: string;
	completed_at: string | null;
	snapshot_at: string | null;
	expires_at: string;
	error_code: string | null;
	total_rows: number;
	counts: {
		new: number;
		created: number;
		unchanged: number;
		conflict: number;
		invalid: number;
		failed: number;
	};
	results: TransferRow[];
	page: number;
	per_page: number;
	byte_size: number;
}
export interface ReauthenticationOptions {
	recent_authentication: boolean;
	password: boolean;
	google: boolean;
	github: boolean;
}

export class TransferError extends Error {
	constructor(
		message: string,
		public code?: string,
		public status?: number
	) {
		super(message);
	}
}

const BASE = '/api/v1/me';
const errors: Record<string, string> = {
	RECENT_AUTH_REQUIRED: 'Bitte bestätige deine Anmeldung erneut.',
	invalid_document: 'Die Datei entspricht nicht dem LILLY-JSON-Format.',
	invalid_utf8: 'Die Datei muss als UTF-8 gespeichert sein.',
	invalid_csv_header: 'Die CSV-Spalten entsprechen nicht der LILLY-Vorlage.',
	invalid_csv_structure: 'Die CSV-Datei enthält fehlerhafte Anführungszeichen oder Datensätze.',
	unsupported_format_version: 'Diese Formatversion wird nicht unterstützt.',
	unsupported_text_encoding: 'Die Textkodierung der CSV-Datei wird nicht unterstützt.',
	preview_expired: 'Die Vorschau ist abgelaufen. Bitte lade die Datei erneut hoch.',
	transfer_active: 'Ein Auftrag läuft bereits. Bitte warte auf dessen Ergebnis.',
	transfer_limit: 'Das tägliche Limit für Datenübertragungen wurde erreicht.',
	export_invalidated:
		'Kontodaten wurden inzwischen gelöscht oder geändert. Bitte fordere einen neuen Export an.',
	export_unavailable: 'Dieser Export ist nicht mehr verfügbar. Bitte fordere ihn erneut an.'
};

async function handle(response: Response): Promise<Response> {
	if (!response.ok) {
		const body = (await response.json().catch(() => ({}))) as { code?: string };
		const message =
			(body.code && errors[body.code]) ||
			(
				{
					401: 'Deine Sitzung ist abgelaufen. Bitte melde dich erneut an.',
					403: 'Bitte bestätige deine Anmeldung erneut.',
					404: 'Dieser Auftrag oder diese Datei ist nicht verfügbar.',
					409: 'Der Auftrag ist nicht mehr verfügbar. Bitte aktualisiere die Ansicht.',
					413: 'Die Datei überschreitet die erlaubte Größe oder Anzahl von Datensätzen.',
					429: 'Zu viele Anfragen. Bitte versuche es später erneut.'
				} as Record<number, string>
			)[response.status] ||
			'Die Datenübertragung ist fehlgeschlagen. Bitte versuche es erneut.';
		throw new TransferError(message, body.code, response.status);
	}
	return response;
}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
	const version = offlineSessionVersion();
	const response = await handle(
		await fetch(`${BASE}${path}`, { credentials: 'same-origin', cache: 'no-store', ...init })
	);
	const result: T = (await response.json()) as T;
	if (!isCurrentOfflineSession(version))
		throw new TransferError('Die Sitzung wurde inzwischen beendet.');
	return result;
}

export async function prepareCollectionTransfer(): Promise<void> {
	if (typeof navigator !== 'undefined' && !navigator.onLine)
		throw new TransferError('Import und Export benötigen eine Internetverbindung.');
	const version = offlineSessionVersion();
	const me = await fetchMe();
	await syncPendingCollectionChanges();
	const [mutations, conflicts] = await Promise.all([listMutations(me.id), listConflicts(me.id)]);
	if (!isCurrentOfflineSession(version))
		throw new TransferError('Die Sitzung wurde inzwischen beendet.');
	if (mutations.length || conflicts.length)
		throw new TransferError(
			'Bitte synchronisiere ausstehende Änderungen und kläre Offline-Konflikte, bevor du Daten überträgst.'
		);
}

export async function previewCollectionImport(
	file: File,
	format: TransferFormat
): Promise<TransferJob> {
	await prepareCollectionTransfer();
	const body = new FormData();
	body.append('file', file);
	return request(`/collection/imports/preview?format=${format}`, { method: 'POST', body });
}
export function fetchCollectionImport(id: string, page = 1): Promise<TransferJob> {
	return request(`/collection/imports/${encodeURIComponent(id)}?page=${page}`);
}
export async function commitCollectionImport(id: string): Promise<TransferJob> {
	await prepareCollectionTransfer();
	return request(`/collection/imports/${encodeURIComponent(id)}/commit`, { method: 'POST' });
}
export async function refreshImportedCollection(): Promise<boolean> {
	try {
		await refreshOfflineSnapshot();
		return true;
	} catch {
		return false;
	}
}

function saveBlob(blob: Blob, filename: string): void {
	const url = URL.createObjectURL(blob);
	const link = document.createElement('a');
	link.href = url;
	link.download = filename;
	document.body.append(link);
	link.click();
	link.remove();
	// Browsers may consume the object URL after the current event handler returns.
	setTimeout(() => URL.revokeObjectURL(url), 1_000);
}
export async function downloadCollection(format: TransferFormat, template = false): Promise<void> {
	if (!template) await prepareCollectionTransfer();
	const version = offlineSessionVersion();
	const response = await handle(
		await fetch(`${BASE}/collection/${template ? 'import/template' : 'export'}?format=${format}`, {
			credentials: 'same-origin',
			cache: 'no-store'
		})
	);
	const blob = await response.blob();
	if (!isCurrentOfflineSession(version))
		throw new TransferError('Die Sitzung wurde inzwischen beendet.');
	saveBlob(blob, `lilly-${template ? 'vorlage' : 'sammlung'}.${format}`);
}

export function fetchReauthenticationOptions(): Promise<ReauthenticationOptions> {
	return request('/reauthentication/options');
}
export function listPersonalExports(page = 1): Promise<TransferJob[]> {
	return request(page === 1 ? '/data-exports' : `/data-exports?page=${page}`);
}
export function requestPersonalExport(): Promise<TransferJob> {
	return request('/data-exports', { method: 'POST' });
}
export function fetchPersonalExport(id: string): Promise<TransferJob> {
	return request(`/data-exports/${encodeURIComponent(id)}`);
}
export async function downloadPersonalExport(id: string): Promise<void> {
	const version = offlineSessionVersion();
	const options = await fetchReauthenticationOptions();
	if (!options.recent_authentication)
		throw new TransferError(errors.RECENT_AUTH_REQUIRED, 'RECENT_AUTH_REQUIRED', 403);
	const job = await fetchPersonalExport(id);
	if (job.status !== 'ready' || Date.parse(job.expires_at) <= Date.now())
		throw new TransferError(errors.export_unavailable, 'export_unavailable', 409);
	if (!isCurrentOfflineSession(version))
		throw new TransferError('Die Sitzung wurde inzwischen beendet.');
	// Native download streams large archives without buffering them in JS memory.
	const link = document.createElement('a');
	link.href = `${BASE}/data-exports/${encodeURIComponent(id)}/download`;
	link.download = `lilly-personal-data-${id}.zip`;
	document.body.append(link);
	link.click();
	link.remove();
}

export const transferStatusLabels: Record<TransferStatus, string> = {
	prepared: 'Vorschau bereit',
	queued: 'In Warteschlange',
	running: 'Wird verarbeitet',
	completed: 'Import abgeschlossen',
	ready: 'Download bereit',
	failed: 'Fehlgeschlagen',
	cancelled: 'Nicht mehr gültig',
	expired: 'Abgelaufen'
};
export const rowStatusLabels: Record<TransferRow['status'], string> = {
	new: 'Neu',
	created: 'Hinzugefügt',
	unchanged: 'Übersprungen',
	conflict: 'Konflikt',
	invalid: 'Ungültig',
	failed: 'Fehlgeschlagen'
};
export function transferRowReason(code: string | null): string {
	return (
		(code &&
			(
				{
					entry_removed: 'Dieses Exemplar wurde bereits entfernt oder abgegeben.',
					job_interrupted:
						'Der Auftrag wurde unterbrochen. Diese Zeile wurde noch nicht hinzugefügt.',
					entry_changed: 'Der vorhandene Eintrag enthält andere Daten und wird erhalten.',
					copy_number_occupied: 'Diese Exemplarnummer gehört bereits zu einem anderen Eintrag.',
					duplicate_file_row: 'Identische Zeile ist bereits in dieser Datei enthalten.',
					duplicate_key_conflict: 'Dieselbe Kennung kommt mit widersprüchlichen Daten vor.',
					issue_not_found: 'Das Heft ist nicht im Katalog vorhanden.',
					series_inactive: 'Die Serie ist noch nicht freigegeben.',
					source_identity_mismatch: 'Die Quellenangaben passen nicht zum angegebenen Heft.',
					invalid_copy_key: 'Die Exemplar-Kennung fehlt oder enthält unzulässige Zeichen.',
					invalid_issue_identity: 'Heftreferenz oder Exemplarnummer ist ungültig.',
					invalid_status_condition: 'Status oder Zustand ist ungültig oder fehlt.',
					invalid_source_identity: 'Die Quellenangaben müssen vollständig angegeben werden.',
					invalid_notes: 'Die Notiz überschreitet die erlaubte Länge.',
					invalid_edition: 'Die Editionsbezeichnung überschreitet die erlaubte Länge.',
					invalid_fields: 'Pflichtfelder fehlen oder enthalten ungültige Werte.'
				} as Record<string, string>
			)[code]) ||
		(code ? 'Dieser Datensatz konnte nicht übernommen werden.' : '')
	);
}
