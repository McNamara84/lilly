export const CONDITION_GRADES = ['Z0', 'Z1', 'Z2', 'Z3', 'Z4'] as const;

export type ConditionGrade = (typeof CONDITION_GRADES)[number];

export interface ConditionDefinition {
	value: ConditionGrade;
	label: string;
	description: string;
}

export const CONDITION_DEFINITIONS: readonly ConditionDefinition[] = [
	{
		value: 'Z0',
		label: 'Druckfrisch',
		description: 'Druckfrisches neues Heft ohne jegliche Mängel. Innenseiten noch weiß.'
	},
	{
		value: 'Z1',
		label: 'Sehr gut erhalten',
		description:
			'Sehr gut erhaltenes Heft mit minimalen Gebrauchsspuren und ohne Risse. Keine Beschriftung oder Aufkleber auf der Titelseite. Heftklammern dürfen leicht angerostet sein, aber ohne Rostflecken auf dem Papier. Kein oder nur minimaler Lesewulst. Innenseiten leicht bräunlich.'
	},
	{
		value: 'Z2',
		label: 'Gut erhalten',
		description:
			'Gut erhaltenes Heft mit Gebrauchsspuren, beispielsweise kleinen Rissen am Rand und leichtem Lesewulst. Klammern angerostet. Keine Beschriftungen oder Aufkleber auf der Titelseite. Innenseiten bräunlich, aber nicht fleckig.'
	},
	{
		value: 'Z3',
		label: 'Stärker beschädigt',
		description:
			'Heft mit stärkeren Beschädigungen, größeren Einrissen und starkem Lesewulst. Klammern angerostet bis verrostet. Geringe Beschriftungen oder Aufkleber auf der Titelseite. Innenseiten stark gedunkelt und fleckig. Das Heft ist noch nicht zerfleddert und hat keine losen oder fehlenden Seiten.'
	},
	{
		value: 'Z4',
		label: 'Stark beschädigt',
		description:
			'Stark beschädigtes Heft. Titelbild eingerissen und/oder deutlich störende Beschriftungen beziehungsweise Aufkleber auf der Titelseite. Innenseiten deutlich braun. Das Heft wirkt zerfleddert und kann lose oder fehlende Seiten haben.'
	}
] as const;

export function isConditionGrade(value: unknown): value is ConditionGrade {
	return typeof value === 'string' && CONDITION_GRADES.includes(value as ConditionGrade);
}

export function getConditionDefinition(
	value: ConditionGrade | null | undefined
): ConditionDefinition | undefined {
	return CONDITION_DEFINITIONS.find((definition) => definition.value === value);
}
