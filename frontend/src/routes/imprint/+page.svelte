<script lang="ts">
	import { resolve } from '$app/paths';
	import type { ImprintConfig } from '$lib/imprint';

	let { data }: { data: { imprint: ImprintConfig } } = $props();
	const imprint = $derived(data.imprint);
	const phoneLines = $derived(imprint.phone ? [`Telefon: ${imprint.phone}`] : []);
	const updatedLines = $derived(imprint.lastUpdated ? [`Stand: ${imprint.lastUpdated}`] : []);
</script>

<svelte:head>
	<title>Impressum – LILLY</title>
</svelte:head>

<div class="min-h-[calc(100vh-3.5rem)] px-4 py-8 sm:px-6 lg:px-8 max-w-3xl mx-auto">
	<h1 class="text-3xl font-bold mb-6" style="color: var(--text-primary);">Impressum</h1>

	<div class="space-y-6 text-sm" style="color: var(--text-secondary);">
		{#if imprint.configured}
			<section data-testid="imprint-operator">
				<h2 class="text-lg font-semibold mb-2" style="color: var(--text-primary);">
					Angaben gemäß § 5 DDG
				</h2>
				<p>
					{imprint.operatorName}{#each imprint.addressLines as line (line)}<br />{line}{/each}
				</p>
			</section>

			<section data-testid="imprint-contact">
				<h2 class="text-lg font-semibold mb-2" style="color: var(--text-primary);">Kontakt</h2>
				<p>
					E-Mail: <a class="underline" href={`mailto:${imprint.email}`}>{imprint.email}</a>
					{#each phoneLines as line (line)}<br />{line}{/each}
				</p>
			</section>

			{#if imprint.responsiblePerson}
				<section data-testid="imprint-responsible">
					<h2 class="text-lg font-semibold mb-2" style="color: var(--text-primary);">
						Verantwortlich für den Inhalt
					</h2>
					<p>{imprint.responsiblePerson}</p>
				</section>
			{/if}

			{#each imprint.additionalParagraphs as paragraph (paragraph)}
				<p>{paragraph}</p>
			{/each}
		{:else}
			<p
				class="rounded-lg border p-4"
				style="border-color: var(--border-default, currentColor);"
				role="status"
				data-testid="imprint-unconfigured"
			>
				Der Betreiber dieser LILLY-Installation hat noch kein Impressum hinterlegt. Betreiber
				konfigurieren es über die Umgebungsvariablen <code>IMPRINT_OPERATOR_NAME</code>,
				<code>IMPRINT_ADDRESS</code> und <code>IMPRINT_EMAIL</code>.
			</p>
		{/if}

		<p>
			Informationen zur Verarbeitung personenbezogener Daten finden Sie in der
			<a class="underline" href={resolve('/privacy')}>Datenschutzerklärung</a>.
		</p>

		{#each updatedLines as line (line)}
			<p data-testid="imprint-last-updated">{line}</p>
		{/each}
	</div>
</div>
