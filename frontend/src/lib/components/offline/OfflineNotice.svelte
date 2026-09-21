<script lang="ts">
	import { getOfflineStatus } from '$lib/offline/status.svelte';

	let { feature }: { feature: string } = $props();
	const status = getOfflineStatus();
</script>

{#if !status.online}
	<p
		class="rounded-lg px-3 py-2 text-sm"
		style="background-color: var(--surface-raised); border: 1px solid var(--glass-border); color: var(--text-secondary);"
		role="status"
		data-testid="offline-unavailable"
	>
		{feature} benötigt eine Internetverbindung. Deine Sammlung bleibt offline nutzbar.
	</p>
{/if}
