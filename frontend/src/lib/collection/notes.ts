export const MAX_COLLECTION_NOTE_LENGTH = 10_000;

export function countCollectionNoteCharacters(note: string): number {
	return Array.from(note).length;
}

export function limitCollectionNote(note: string): string {
	return Array.from(note).slice(0, MAX_COLLECTION_NOTE_LENGTH).join('');
}
