<script lang="ts">
	import { resolve } from '$app/paths';
	import { fetchAuthOptions } from '$lib/api/auth';

	let policyVersion = $state('');

	$effect(() => {
		void fetchAuthOptions()
			.then((options) => (policyVersion = options.privacy_policy.version))
			.catch(() => (policyVersion = ''));
	});
</script>

<svelte:head>
	<title>Datenschutzerklärung – LILLY</title>
</svelte:head>

<div class="min-h-[calc(100vh-3.5rem)] px-4 py-8 sm:px-6 lg:px-8 max-w-3xl mx-auto">
	<h1 class="text-3xl font-bold mb-6" style="color: var(--text-primary);">Datenschutzerklärung</h1>
	<p class="mb-6 text-sm" data-testid="privacy-policy-version">
		Version {policyVersion || 'wird geladen'}
	</p>

	<div class="space-y-6 text-sm" style="color: var(--text-secondary);">
		<section>
			<h2 class="text-lg font-semibold mb-2" style="color: var(--text-primary);">
				1. Verantwortlicher
			</h2>
			<p>
				Verantwortlich für die Datenverarbeitung auf dieser Plattform ist der Betreiber von LILLY.
				Kontaktdaten können über die Plattform angefragt werden.
			</p>
		</section>

		<section>
			<h2 class="text-lg font-semibold mb-2" style="color: var(--text-primary);">
				2. Erhobene Daten
			</h2>
			<p>
				Bei der Registrierung und Nutzung von LILLY werden folgende personenbezogene Daten erhoben:
			</p>
			<ul class="list-disc list-inside mt-2 space-y-1">
				<li>Anzeigename</li>
				<li>E-Mail-Adresse</li>
				<li>Passwort-Hash (Argon2id; das Klartextpasswort wird nicht gespeichert)</li>
				<li>Version und Zeitpunkt der Datenschutz-Einwilligung sowie Registrierungsweg</li>
				<li>
					Sammlungsdaten (Serien, Hefte, Editionen, Tauschangebote, persönliche Heftnotizen und
					hochgeladene Fotos)
				</li>
				<li>
					Tausch-Matches, Tauschvorschläge, Abschlussbestätigungen und zugehörige interne
					Nachrichten
				</li>
			</ul>
		</section>

		<section>
			<h2 class="text-lg font-semibold mb-2" style="color: var(--text-primary);">
				3. Zweck der Verarbeitung
			</h2>
			<p>Die erhobenen Daten werden ausschließlich für folgende Zwecke verwendet:</p>
			<ul class="list-disc list-inside mt-2 space-y-1">
				<li>Bereitstellung und Verwaltung des Benutzerkontos</li>
				<li>Verwaltung der persönlichen Heftroman-Sammlung</li>
				<li>Ermöglichung des Tauschs zwischen Nutzern</li>
				<li>Benachrichtigung und private Abstimmung zu konkreten Tauschvorgängen</li>
				<li>Versand von E-Mail-Benachrichtigungen (z.B. E-Mail-Verifizierung)</li>
			</ul>
		</section>

		<section>
			<h2 class="text-lg font-semibold mb-2" style="color: var(--text-primary);">
				4. Anmeldung über Google oder GitHub
			</h2>
			<p>
				Optional kann die Registrierung und Anmeldung über Google oder GitHub erfolgen. LILLY fragt
				bei Google ausschließlich die Berechtigungen <code>openid</code>, <code>email</code>
				und <code>profile</code> sowie bei GitHub ausschließlich <code>read:user</code> und
				<code>user:email</code> an. Verarbeitet werden die stabile Anbieter-ID, eine bestätigte primäre
				E-Mail-Adresse und ein Anzeigename. Zugriffstoken werden nur kurzzeitig für den Anmeldevorgang
				verwendet und nicht gespeichert. Eine neue Anbieter-Identität wird bei übereinstimmender E-Mail
				erst nach Anmeldung am bestehenden LILLY-Konto und ausdrücklicher Bestätigung verknüpft.
			</p>
		</section>

		<section>
			<h2 class="text-lg font-semibold mb-2" style="color: var(--text-primary);">
				5. Öffentliche Freigaben
			</h2>
			<p>
				Profil und Sammlung sind standardmäßig privat und können unabhängig voneinander öffentlich
				freigegeben werden. Bei einer öffentlichen Sammlung werden auch die darin gespeicherten
				persönlichen Heftnotizen öffentlich angezeigt. Die Freigabe kann jederzeit in den
				Profileinstellungen zurückgenommen werden.
			</p>
		</section>

		<section>
			<h2 class="text-lg font-semibold mb-2" style="color: var(--text-primary);">
				6. Rechtsgrundlage
			</h2>
			<p>
				Die Verarbeitung erfolgt auf Grundlage Ihrer Einwilligung (Art. 6 Abs. 1 lit. a DSGVO) sowie
				zur Erfüllung des Nutzungsvertrags (Art. 6 Abs. 1 lit. b DSGVO).
			</p>
		</section>

		<section>
			<h2 class="text-lg font-semibold mb-2" style="color: var(--text-primary);">
				7. Speicherdauer
			</h2>
			<p>
				Ihre Daten werden so lange gespeichert, wie Ihr Benutzerkonto besteht. Nach Löschung des
				Kontos werden alle personenbezogenen Daten entfernt, sofern keine gesetzlichen
				Aufbewahrungspflichten bestehen. Beim Abschluss eines Tauschs werden persönliche Notizen und
				Fotos eines abgegebenen Exemplars nicht an den Empfänger übertragen; dessen Fotos werden
				gelöscht. Nachrichten bleiben unabhängig vom Tauschstatus bis zur Löschung eines beteiligten
				Kontos erhalten; dann wird der gemeinsame Tausch- und Nachrichtenkontext gelöscht.
			</p>
		</section>

		<section>
			<h2 class="text-lg font-semibold mb-2" style="color: var(--text-primary);">8. Ihre Rechte</h2>
			<p>Sie haben das Recht auf:</p>
			<ul class="list-disc list-inside mt-2 space-y-1">
				<li>Auskunft über Ihre gespeicherten Daten</li>
				<li>Berichtigung unrichtiger Daten</li>
				<li>Löschung Ihrer Daten</li>
				<li>Einschränkung der Verarbeitung</li>
				<li>Datenübertragbarkeit</li>
				<li>Widerruf Ihrer Einwilligung</li>
			</ul>
		</section>

		<section>
			<h2 class="text-lg font-semibold mb-2" style="color: var(--text-primary);">9. Cookies</h2>
			<p>
				LILLY verwendet ausschließlich technisch notwendige Cookies zur Authentifizierung (HttpOnly,
				SameSite). Es werden keine Tracking- oder Analyse-Cookies eingesetzt.
			</p>
		</section>

		<section>
			<h2 class="text-lg font-semibold mb-2" style="color: var(--text-primary);">10. Kontakt</h2>
			<p>
				Bei Fragen zum Datenschutz wenden Sie sich bitte an den Betreiber über die auf der Plattform
				bereitgestellten Kontaktmöglichkeiten.
			</p>
		</section>
	</div>

	<div class="mt-8">
		<a
			href={resolve('/register')}
			class="text-sm font-medium"
			style="color: var(--color-brand-500);"
		>
			← Zurück zur Registrierung
		</a>
	</div>
</div>
