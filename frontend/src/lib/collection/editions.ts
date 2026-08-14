export const MAX_EDITION_LABEL_LENGTH = 120;

export function countEditionLabelCharacters(value: string): number {
	return Array.from(value).length;
}

export function limitEditionLabel(value: string): string {
	return Array.from(value).slice(0, MAX_EDITION_LABEL_LENGTH).join('');
}
