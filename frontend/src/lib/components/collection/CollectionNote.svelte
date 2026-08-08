<script lang="ts">
	interface Props {
		note: string | null;
		emptyText?: string;
	}

	let { note, emptyText = 'Keine öffentliche Notiz.' }: Props = $props();
</script>

{#if note}
	<p
		class="whitespace-pre-wrap break-words text-sm"
		style="color: var(--text-secondary);"
		data-testid="collection-note"
	>
		{note}
	</p>
{:else}
	<p class="text-sm" style="color: var(--text-tertiary);" data-testid="collection-note-empty">
		{emptyText}
	</p>
{/if}
