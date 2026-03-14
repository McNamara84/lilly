<script lang="ts">
	import { getAuthState, performLogout } from '$lib/stores/auth.svelte';
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';

	const auth = getAuthState();

	async function handleLogout() {
		await performLogout();
		await goto(resolve('/login'));
	}
</script>

<svelte:head>
	<title>Übersicht – LILLY</title>
</svelte:head>

<div class="min-h-[calc(100vh-3.5rem)] px-4 py-8 sm:px-6 lg:px-8">
	{#if auth.user}
		<!-- Welcome Header -->
		<div class="mb-8" data-testid="welcome-header">
			<h2 class="text-2xl font-bold" style="color: var(--text-primary);">
				Willkommen zurück, {auth.user.display_name}!
			</h2>
			<p class="text-sm mt-1" style="color: var(--text-secondary);">
				{new Date().toLocaleDateString('de-DE', {
					weekday: 'long',
					day: 'numeric',
					month: 'long',
					year: 'numeric'
				})}
			</p>
		</div>

		<!-- Stats Cards -->
		<div class="grid grid-cols-2 lg:grid-cols-4 gap-4 mb-8" data-testid="stats-row">
			<div class="glass-elevated p-4 rounded-lg">
				<p class="text-xs mb-1" style="color: var(--text-tertiary);">Gesamte Hefte</p>
				<p class="text-2xl font-bold" style="color: var(--text-primary);">0</p>
			</div>
			<div class="glass-elevated p-4 rounded-lg">
				<p class="text-xs mb-1" style="color: var(--text-tertiary);">Sammlungsfortschritt</p>
				<p class="text-2xl font-bold" style="color: var(--text-primary);">0%</p>
			</div>
			<div class="glass-elevated p-4 rounded-lg">
				<p class="text-xs mb-1" style="color: var(--text-tertiary);">Tauschbar</p>
				<p class="text-2xl font-bold" style="color: var(--text-primary);">0</p>
			</div>
			<div class="glass-elevated p-4 rounded-lg">
				<p class="text-xs mb-1" style="color: var(--text-tertiary);">Gesucht</p>
				<p class="text-2xl font-bold" style="color: var(--text-primary);">0</p>
			</div>
		</div>

		<!-- Empty State -->
		<div class="glass-elevated p-8 rounded-lg text-center" data-testid="empty-state">
			<p class="text-lg font-medium mb-2" style="color: var(--text-primary);">
				Deine Sammlung ist noch leer
			</p>
			<p class="text-sm" style="color: var(--text-secondary);">
				Beginne damit, Serien und Hefte zu deiner Sammlung hinzuzufügen.
			</p>
		</div>

		<!-- Logout -->
		<div class="mt-8 text-center">
			<button
				onclick={handleLogout}
				class="rounded-lg px-6 py-2.5 text-sm font-medium transition-colors cursor-pointer"
				style="background-color: var(--surface-raised); border: 1px solid var(--glass-border); color: var(--text-secondary);"
				data-testid="logout-button"
			>
				Abmelden
			</button>
		</div>
	{/if}
</div>
