<script lang="ts">
	import OnlineOnly from '$lib/components/offline/OnlineOnly.svelte';

	let { children } = $props();
</script>

<OnlineOnly feature="Tauschabgleich">
	{@render children()}
</OnlineOnly>
