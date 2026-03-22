<script lang="ts">
	const GRADES = [
		{ value: 'Z0', label: 'Neuwertig' },
		{ value: 'Z1', label: 'Sehr gut' },
		{ value: 'Z2', label: 'Gut' },
		{ value: 'Z3', label: 'Akzeptabel' },
		{ value: 'Z4', label: 'Schlecht' },
		{ value: 'Z5', label: 'Sehr schlecht' }
	] as const;

	interface Props {
		value: string | null;
		onchange: (grade: string) => void;
		disabled?: boolean;
	}

	let { value, onchange, disabled = false }: Props = $props();
</script>

<fieldset class="flex flex-wrap gap-2" aria-label="Zustandsbewertung" data-testid="condition-chips">
	{#each GRADES as grade (grade.value)}
		{@const selected = value === grade.value}
		<button
			type="button"
			class="flex flex-col items-center px-3 py-1.5 rounded-lg text-sm transition-all cursor-pointer"
			class:opacity-50={disabled}
			style={selected
				? `background-color: var(--color-condition-${grade.value.toLowerCase()}); color: #000; box-shadow: 0 0 12px var(--color-condition-${grade.value.toLowerCase()});`
				: `background: var(--glass); border: 1px solid var(--glass-border); color: var(--text-secondary);`}
			aria-pressed={selected}
			{disabled}
			onclick={() => onchange(grade.value)}
			data-testid={`condition-chip-${grade.value}`}
		>
			<span class="font-bold">{grade.value}</span>
			<span class="text-[10px] leading-tight">{grade.label}</span>
		</button>
	{/each}
</fieldset>
