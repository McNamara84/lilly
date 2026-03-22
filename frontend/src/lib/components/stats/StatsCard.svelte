<script lang="ts">
	interface Props {
		label: string;
		value: string | number;
		icon?: string;
	}

	let { label, value, icon }: Props = $props();
</script>

<div class="glass-elevated p-4 rounded-lg relative overflow-hidden" data-testid="stats-card">
	{#if icon}
		<span
			class="absolute top-2 right-2 text-5xl opacity-20 select-none"
			style="color: var(--color-brand-500);"
			aria-hidden="true"
		>
			{icon}
		</span>
	{/if}
	<p class="text-xs mb-1" style="color: var(--text-tertiary);">{label}</p>
	<p class="text-2xl font-bold" style="color: var(--text-primary);">{value}</p>
</div>
