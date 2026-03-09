<script lang="ts">
	import '../app.css';

	let { children } = $props();

	let theme = $state<'dark' | 'light'>('dark');

	function toggleTheme() {
		theme = theme === 'dark' ? 'light' : 'dark';
		document.documentElement.setAttribute('data-theme', theme);
	}

	$effect(() => {
		const prefersDark = window.matchMedia('(prefers-color-scheme: dark)').matches;
		theme = prefersDark ? 'dark' : 'light';
		document.documentElement.setAttribute('data-theme', theme);
	});
</script>

<svelte:head>
	<title>LILLY – Listing Inventory for Lovely Little Yellowbacks</title>
</svelte:head>

<div
	class="min-h-screen"
	style="background-color: var(--surface-base); color: var(--text-primary);"
>
	<header
		class="glass-nav fixed top-0 left-0 right-0 z-50 h-14 flex items-center justify-between px-4"
	>
		<span class="text-xl font-bold tracking-tight" style="color: var(--color-brand-500);">
			LILLY
		</span>
		<button
			onclick={toggleTheme}
			class="p-2 rounded-lg transition-colors cursor-pointer"
			style="color: var(--text-secondary);"
			aria-label={theme === 'dark' ? 'Zum hellen Modus wechseln' : 'Zum dunklen Modus wechseln'}
		>
			{#if theme === 'dark'}
				<svg
					xmlns="http://www.w3.org/2000/svg"
					width="20"
					height="20"
					viewBox="0 0 24 24"
					fill="none"
					stroke="currentColor"
					stroke-width="2"
					stroke-linecap="round"
					stroke-linejoin="round"
					><circle cx="12" cy="12" r="4" /><path d="M12 2v2" /><path d="M12 20v2" /><path
						d="m4.93 4.93 1.41 1.41"
					/><path d="m17.66 17.66 1.41 1.41" /><path d="M2 12h2" /><path d="M20 12h2" /><path
						d="m6.34 17.66-1.41 1.41"
					/><path d="m19.07 4.93-1.41 1.41" /></svg
				>
			{:else}
				<svg
					xmlns="http://www.w3.org/2000/svg"
					width="20"
					height="20"
					viewBox="0 0 24 24"
					fill="none"
					stroke="currentColor"
					stroke-width="2"
					stroke-linecap="round"
					stroke-linejoin="round"><path d="M12 3a6 6 0 0 0 9 9 9 9 0 1 1-9-9Z" /></svg
				>
			{/if}
		</button>
	</header>

	<main class="pt-14">
		{@render children()}
	</main>
</div>
