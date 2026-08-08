<script lang="ts">
	import { CONDITION_DEFINITIONS, type ConditionGrade } from '$lib/collection/conditions';

	interface Props {
		value: ConditionGrade | null;
		onchange: (grade: ConditionGrade) => void;
		disabled?: boolean;
	}

	let { value, onchange, disabled = false }: Props = $props();
</script>

<fieldset class="flex flex-wrap gap-2" aria-label="Zustandsbewertung" data-testid="condition-chips">
	{#each CONDITION_DEFINITIONS as grade (grade.value)}
		{@const selected = value === grade.value}
		<button
			type="button"
			class="flex flex-col items-center px-3 py-1.5 rounded-lg text-sm transition-all cursor-pointer"
			class:opacity-50={disabled}
			style={selected
				? `background-color: var(--color-condition-${grade.value.toLowerCase()}); color: #000; box-shadow: 0 0 12px var(--color-condition-${grade.value.toLowerCase()});`
				: `background: var(--glass); border: 1px solid var(--glass-border); color: var(--text-secondary);`}
			aria-pressed={selected}
			aria-label={`${grade.value}: ${grade.label}. ${grade.description}`}
			title={grade.description}
			{disabled}
			onclick={() => onchange(grade.value)}
			data-testid={`condition-chip-${grade.value}`}
		>
			<span class="font-bold">{grade.value}</span>
			<span class="text-[10px] leading-tight">{grade.label}</span>
		</button>
	{/each}
</fieldset>
