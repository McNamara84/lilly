import { zxcvbn, zxcvbnOptions } from '@zxcvbn-ts/core';
import * as zxcvbnCommonPackage from '@zxcvbn-ts/language-common';
import * as zxcvbnDePackage from '@zxcvbn-ts/language-de';

const options = {
	translations: zxcvbnDePackage.translations,
	graphs: zxcvbnCommonPackage.adjacencyGraphs,
	dictionary: {
		...zxcvbnCommonPackage.dictionary,
		...zxcvbnDePackage.dictionary
	}
};

zxcvbnOptions.setOptions(options);

export interface PasswordStrengthResult {
	score: 0 | 1 | 2 | 3 | 4;
	label: string;
	color: string;
}

const LABELS: Record<number, string> = {
	0: 'Sehr schwach',
	1: 'Schwach',
	2: 'Ausreichend',
	3: 'Stark',
	4: 'Sehr stark'
};

const COLORS: Record<number, string> = {
	0: '#ef4444',
	1: '#f97316',
	2: '#eab308',
	3: '#84cc16',
	4: '#22c55e'
};

export function checkPasswordStrength(
	password: string,
	userInputs: string[] = []
): PasswordStrengthResult {
	if (!password) {
		return { score: 0, label: LABELS[0], color: COLORS[0] };
	}

	const result = zxcvbn(password, userInputs);
	const score = result.score as 0 | 1 | 2 | 3 | 4;

	return {
		score,
		label: LABELS[score],
		color: COLORS[score]
	};
}

export const MIN_SCORE = 2;
