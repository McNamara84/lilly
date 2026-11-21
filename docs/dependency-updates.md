# Abhängigkeits- und Sicherheitsstand vom 21. November 2026

Stabile Versionen wurden gegen npm, crates.io, PyPI, die GitHub-Releases der
Hersteller und die Docker-Registry geprüft. Direkte Abhängigkeiten und die
kompatibel auflösbaren indirekten Abhängigkeiten sind aktualisiert. GitHub Actions
bleiben mit vollständigen Commit-SHAs, externe Images mit verifizierten
Multiarch-Digests fixiert. Die unten beschriebene Playwright-Vorabversion ist eine
gezielte Ausnahme für einen bestätigten Fehler des mitgelieferten Firefox-Builds.

## Aktuelle Versionen

| Bereich | Stand |
| --- | --- |
| Node.js / npm | 26.13.1 LTS / 12.2.3 |
| Rust | 1.100.0, lokal, in CI und im Slim-Trixie-Buildimage |
| Svelte / SvelteKit / Node-Adapter | 5.59.1 / 3.4.0 / 6.0.5 |
| Tailwind CSS / Skeleton Svelte | 4.3.5 / 5.0.6 |
| Vite / Vitest / Playwright | 8.3.8 / 5.1.1 / 1.66.0-alpha-2026-11-21, exakt gepinnt |
| Jest-DOM / jsdom | 8.0.0 / 31.0.1 |
| TypeScript: nativer Compiler / JavaScript-Compiler-API | 7.0.2 / 6.0.3 |
| Lighthouse / Lighthouse CI | 13.6.0 / 0.16.1 |
| SQLx / jsonwebtoken / jsonschema | 0.9.1 / 12.0.0 / 0.62.0 |
| Python-Zeitzonendaten | tzdata 2026.6, für Windows festgelegt |
| MariaDB | 13.0.2, stabiles Rolling Release, mit verifiziertem Multiarch-Digest |
| Caddy / Mailpit | 2.11.9 / 1.34.0 |
| Debian-Laufzeitimage | Trixie Slim mit erneuertem Image-Digest |

Docker Login wurde auf
[4.8.0](https://github.com/docker/login-action/releases/tag/v4.8.0) aktualisiert,
der Rust-Toolchain-Installer auf den geprüften Commit vom 15. November 2026.
Die übrigen verwendeten Actions entsprechen bereits ihren neuesten stabilen
Releases; ihre Commit-Pins wurden gegen die Release-Tags geprüft.

## Korrektur des Firefox-CI-Absturzes

Der [CI-Lauf auf `6e72bba`](https://github.com/McNamara84/lilly/actions/runs/44897987894)
scheiterte ausschließlich in der Firefox-Barrierefreiheitsprüfung der Profilseite.
Der Browser stürzte auch bei beiden Wiederholungen ab. Ein unveränderter lokaler
Linux-Durchlauf mit Firefox-Build 1572 reproduzierte drei Abstürze in 20 Versuchen;
ein einzelner grüner Gesamtlauf reichte deshalb nicht als Entwarnung.

Ursache ist [Mozilla-Bug 2090055](https://bugzilla.mozilla.org/show_bug.cgi?id=2090055):
Beim Lesen von ARIA-Elementreferenzen wie `ariaLabelledByElements` aus dem
Seitenkontext und Playwrights isolierter Skriptumgebung kann Firefox durch einen
Nullzeigerzugriff abstürzen. Die [Playwright-Diagnose](https://github.com/microsoft/playwright/issues/44135#issuecomment-6918863177)
bestätigt den Fehler in 1.65.1 und die Korrektur in neueren Firefox-Builds.
Die Browser werden in CI bereits frisch installiert; ein alter Browser-Cache war
daher nicht die Ursache.

Nach ausdrücklicher Freigabe ist `@playwright/test` vorübergehend exakt auf
`1.66.0-alpha-2026-11-21` festgelegt.
Diese Version enthält den korrigierten Firefox-Build 1578. Die bestehende
Browserinstallation in CI übernimmt damit automatisch die passenden Builds.
Die Ausnahme soll entfallen, sobald eine stabile Playwright-Version den Fix enthält.
Timeouts, Wiederholungen, Trace-Aufzeichnung und Axe-Prüfregeln bleiben unverändert.
Ein gezielter npm-Override bindet auch Axes `playwright-core`-Peer an dieselbe
Version über `$@playwright/test`. Ohne diese Auflösung schließt der Peer-Bereich
die Vorabversion aus und npm installiert zusätzlich den stabilen Core mit
abweichenden `Page`-Typen. Override und Vorabversions-Pin sollen gemeinsam mit dem
Wechsel zurück zu einer stabilen Version entfallen.

Ein kleiner zusätzlicher E2E-Regressionstest liest die ARIA-Referenzen mit Axe und
anschließend über benannte Playwright-Lokatoren. Er stürzt mit Build 1572 ab und
besteht mit Build 1578. Der ursprüngliche Profiltest bestand mit dem korrigierten
Browser außerdem 20 von 20 Durchläufen ohne Wiederholung.

Die freigegebene Version verwendet Chromium 159.0.8154.13 (Build 1256),
Firefox 159.0 (Build 1578) und WebKit 27.2 (Build 2395). Die passenden Browser sind
auch im regulären lokalen Windows-Cache installiert.

Nachprüfung mit der endgültigen Lockdatei:

- `npm ci` unter Windows und Linux erfolgreich; alle Playwright-Pakete und Axes
  Peer verwenden dieselbe Version. npm meldet keine Schwachstellen.
- TypeScript-7-Prüfung, ESLint und Prettier erfolgreich. Der Frontend-Docker-Build
  einschließlich `npm ci`, Typprüfung und Produktionsbuild ist erfolgreich.
- Alle 1.027 Unit-Tests erfolgreich; Statement-Coverage unverändert 98,68 %.
- Vollständige Linux-Suites ohne Wiederholung erfolgreich: Chromium 107,
  Firefox 107 und WebKit 106 Tests. Die drei Produktionsupdate-Fälle je Browser
  benötigen einen gesonderten Zwei-Build-Lauf; WebKit überspringt zusätzlich den
  bestehenden Offline-Navigationstest.
- Alle zwölf mobilen Chromium-PWA-Tests erfolgreich.
- Alle neun Update-Prüfungen zwischen zwei Produktionsbuilds in Chromium,
  Firefox und WebKit ohne Wiederholung erfolgreich.
- Alle zehn Lighthouse-Messungen und die unveränderten Grenzwerte erfolgreich.
  Median für den öffentlichen Einstieg: Performance 99/100, FCP 762 ms;
  für die angemeldete Sammlung: Performance 95/100, FCP 758 ms.
- Der neue Browser-Regressionstest besteht zusätzlich unter Windows in allen
  drei Browsern mit dem regulären Playwright-Cache.

## Freigegebene Migrationen und Ausnahmen

Die fünf vorab vorgeschlagenen Versionssprünge wurden ausdrücklich freigegeben:
Jest-DOM 8, jsdom 31, Lighthouse CI 0.16.1 einschließlich uuid 14,
jsonwebtoken 12 und jsonschema 0.62.0.

Bei jsonwebtoken liefern HMAC-Schlüsselkonstruktoren jetzt ein `Result` zurück.
Die Anwendung reicht Fehler an ihren bisherigen Fehlerpfad weiter. Zwei zusätzliche
Tests sichern ab, dass leere Schlüssel beim Ausstellen und Prüfen eines Tokens
kontrolliert abgelehnt werden. Siehe den
[Changelog von jsonwebtoken 12](https://docs.rs/crate/jsonwebtoken/12.0.0/source/CHANGELOG.md).

Die neue Clippy-Version verlangt an sechs Stellen `map_or_default` statt
`map(...).unwrap_or_default()`. Der noch funktionslose Importer-Platzhalter hatte
zwei unbenutzte Abhängigkeiten; diese wurden nach den neuen Cargo-Warnungen entfernt.
Die aktualisierten Prettier-Versionen erfordern zusätzlich reine
Formatierungsänderungen in 69 Frontend-Dateien.

Die WebKit-Prüfung machte außerdem einen verlorenen Klick auf den Historie-Tab
sichtbar: Die Schaltflächen waren bereits vor Abschluss der clientseitigen
Initialisierung bedienbar. Die drei Tausch-Tabs bleiben jetzt über den bestehenden
Ladezustand deaktiviert, bis die Daten und Ereignisbehandlung bereit sind. Eine
gezielte Regressionprüfung hält die erste API-Antwort zurück und prüft anschließend
die erfolgreiche Auswahl der Historie.

Die bisherigen Sicherheits-Overrides für npm sind entfallen: Lighthouse CI verwendet nun selbst
Lighthouse 13.6.0 und uuid 14.0.3. `tmp` kommt im aufgelösten Baum nicht mehr vor;
`get-uri` erlaubt bereits die installierte neueste Version von `basic-ftp`.
Der aufgelöste Paketbaum blieb beim Entfernen der letzten Overrides unverändert.

Die anschließend ebenfalls freigegebenen Umstellungen auf MariaDB 13 und den
parallelen Betrieb von TypeScript 7 sind nun umgesetzt:

- **MariaDB 13.0.2:** Entwicklung, Acceptance, E2E, Produktions-Compose und beide
  CI-Datenbankdienste verwenden denselben verifizierten Image-Pin.
  `MARIADB_AUTO_UPGRADE=1` aktiviert beim Wechsel eines vorhandenen Datenvolumes
  die Aktualisierung der Systemtabellen einschließlich des offiziellen
  Systemdatenbank-Backups. Das Deployment erstellt davor weiterhin sein separates
  Anwendungsbackup. Der Community-Support der 13.0-Linie endet laut
  [MariaDB-Supportübersicht](https://mariadb.org/about/) im vierten Quartal 2026;
  Folgeupdates der Rolling-Release-Linie sind entsprechend einzuplanen.
- **TypeScript 7.0.2 und 6.0.3:** `typescript` bleibt für die von SvelteKit und
  ESLint benötigte JavaScript-Compiler-API auf 6.0.3. Der zusätzliche npm-Alias
  `@typescript/native-preview: npm:typescript@^7.0.2` verweist auf das stabile
  TypeScript-Paket. Der Aliasname vermeidet die optionale `@typescript/native`-
  Peer-Abhängigkeit des aktuellen ESLint-Parsers, die bereits bestimmte
  experimentelle 7.1-Versionen verlangt. Es werden keine Vorabversionen und keine
  Peer-Overrides benötigt. Siehe auch
  [Microsofts Anleitung zum Parallelbetrieb](https://devblogs.microsoft.com/typescript/announcing-typescript-7-0/).
- `npm run check` prüft Svelte-Dateien über `svelte-check --tsgo` sowie den
  separaten Service Worker über den expliziten TypeScript-7-CLI-Pfad. Beide
  bestehenden CI-Workflows und der Frontend-Docker-Build führen diese Prüfung aus.
  Drei Callback-Parameter wurden für die native Svelte-Typprüfung ausdrücklich
  typisiert; die Laufzeitlogik bleibt dabei gleich.
- Ein fehlgeschlagenes Deployment setzt ausschließlich die Anwendungscontainer
  `backend`, `frontend` und `caddy` mit `--no-deps` zurück. Das bereits aktualisierte
  Datenverzeichnis wird dadurch nicht mit einem älteren Datenbankimage gestartet.
  Die Anleitung für manuelle Rollbacks unterscheidet Anwendungsrollback und
  Wiederherstellung des Datenbankservers auf einem frischen Volume.

Die erste Review-Runde zeigte einen anschließenden Recovery-Fehler: `current`
bleibt beim Anwendungsrollback auf dem alten Release, dessen Restore-Skript das
frühere Datenbankimage starten würde. `deploy.sh` hält deshalb vor dem
Containerstart einen unabhängigen Verweis `infrastructure` auf das versuchte
Release fest. Dieser bleibt beim Rollback erhalten. Die dokumentierte Recovery
verwendet dessen Restore-Skript und Compose-Datei. Der Restore sperrt parallele
Deployments und lehnt ein vom laufenden Container abweichendes Datenbankimage
einschließlich abweichender Digests vor allen Dienste- und Datenänderungen ab.
Alle 25 Python-/Bash-Regressionstests unter Linux sind erfolgreich, einschließlich
der beiden Rollback-Fälle und neun zusätzlicher Recovery-Tests.

Die anschließende Review-Runde ergänzte den gegenseitigen Ausschluss von Backup
und Restore: Restore erwirbt zuerst `shared/.deploy.lock` und anschließend
`backups/.backup.lock`, in derselben Reihenfolge wie ein Deployment mit seinem
Pre-Deployment-Backup. Beide Sperren bleiben bis zum Abschluss oder Abbruch des
Restores gehalten. Ein laufendes Backup wird abgewartet; manuelle und geplante
Backups brechen während eines Restores vor ihren Datenzugriffen ab. Alle 28
Linux-Regressionstests sind erfolgreich. Die Nebenläufigkeitstests verwenden
echtes `flock`, prüfen beide Aufrufreihenfolgen und die Freigabe der Sperren nach
erfolgreichem sowie fehlgeschlagenem Restore.

## Nachprüfung mit MariaDB 13 und TypeScript 7

- `npm ci` und der vollständige installierte npm-Abhängigkeitsbaum sind konsistent;
  npm meldet keine Schwachstellen. Die Plattformpakete des nativen Compilers sind
  auch für Linux in der Lockdatei erfasst.
- Svelte- und Service-Worker-Typprüfung mit TypeScript 7.0.2 erfolgreich, unter
  Windows und im Linux-Frontend-Build; SvelteKit und ESLint nutzen weiterhin die
  TypeScript-6.0.3-API. ESLint und Prettier erfolgreich.
- Alle 1.027 Frontend-Tests erfolgreich; Statement-Coverage 98,68 %.
- Rustfmt, Clippy für alle Ziele mit `-D warnings` und alle 438 Rust-Tests
  einschließlich Datenbanktests gegen eine eigene MariaDB-13-Datenbank erfolgreich.
- Alle 16 Python-Tests unter Linux erfolgreich, einschließlich zweier neuer
  Deployment-Regressionstests für Fehler beim Containerstart und bei der
  anschließenden HTTP-Gesundheitsprüfung. Beide sichern den Verbleib der
  Datenbankversion beim automatischen Anwendungsrollback ab. Der Start nur der
  Anwendungsdienste mit `--no-deps` wurde auch im echten Docker-Teststack geprüft;
  die ID des laufenden Datenbankcontainers blieb dabei unverändert.
- Echtes Upgrade desselben isolierten Testvolumes von MariaDB 12.3.3 auf 13.0.2
  erfolgreich: alle 35 Anwendungstabellen, 33 SQLx-Migrationseinträge und sämtliche
  gesicherten Anwendungszeilen unverändert. Eine bestehende Sitzung und die
  Sammlung mit Unicode-Notiz funktionieren nach dem Upgrade weiter. Das offizielle
  Systemdatenbank-Backup und der Upgrade-Marker für Version 13.0.2 wurden geprüft.
- Der vor dem Upgrade gesicherte Datenbestand wurde in einem zweiten, frischen
  MariaDB-12.3-Volume wiederhergestellt. Der Vergleich aller gespeicherten
  Anwendungszeilen stimmt sowohl nach dem Upgrade als auch nach der
  Wiederherstellung überein.
- Backend- und Frontend-Container-Builds sowie Produktions- und Acceptance-
  Compose-Validierung erfolgreich. Alle fünf Dienste des isolierten E2E-Stacks
  wurden gesund gestartet.
- Chromium mit MariaDB 13: 106 Browserprüfungen ohne Wiederholung erfolgreich;
  drei Prüfungen für den gesonderten Wechsel zwischen zwei Produktionsbuilds
  wurden wie vorgesehen übersprungen. Die frühere vollständige Browsermatrix und
  PWA-/Lighthouse-Prüfung sind nachfolgend separat dokumentiert.

## Prüfung vor der zusätzlichen MariaDB-/TypeScript-Freigabe

- `npm ci` bestätigt die Konsistenz von Manifest und Lockdatei.
- ESLint, Prettier, Svelte-Typprüfung und separate Service-Worker-Typprüfung erfolgreich.
- 1.027 Frontend-Tests erfolgreich; Statement-Coverage 98,68 %.
  Ein erster Lauf unter paralleler Build-Last überschritt in einem Test das
  Fünf-Sekunden-Limit. Die betroffene Datei bestand isoliert mit 34 Tests;
  anschließend bestand der gesamte Coverage-Lauf mit vier Workern.
- Rustfmt und Clippy für alle Workspace-Ziele mit `-D warnings` erfolgreich.
- 438 Rust-Tests einschließlich der Datenbanktests gegen die isolierte MariaDB
  erfolgreich. Der generierte OpenAPI-Vertrag ist unverändert aktuell.
- 14 Python-Tests mit tzdata 2026.6 erfolgreich.
- Backend- und Frontend-Docker-Build erfolgreich; alle fünf Dienste des isolierten
  E2E-Stacks gesund.
- Chromium: 106 Browserprüfungen ohne Wiederholung erfolgreich;
  drei Update-Prüfungen benötigen den gesonderten Lauf mit zwei Produktionsbuilds.
- Firefox: im abschließenden Gesamtlauf 106 Browserprüfungen ohne Wiederholung
  erfolgreich. In einem früheren Lauf stürzte einmal der Browserprozess bei der
  Barrierefreiheitsprüfung der Profilseite ab; diese Prüfung bestand anschließend
  außerdem dreimal isoliert ohne Wiederholung.
- WebKit: nach der Tab-Korrektur 105 Browserprüfungen ohne Wiederholung erfolgreich.
  Zusätzlich zu den drei gesonderten Update-Prüfungen wird ein bestehender
  Offline-Navigationstest wegen einer Playwright-WebKit-Einschränkung übersprungen.
- Alle drei Chromium-Tauschprüfungen nach der Tab-Korrektur erneut erfolgreich.
- Alle zwölf mobilen Chromium-Prüfungen für Offline-Nutzung und Synchronisierung
  erfolgreich.
- Alle neun Update-Prüfungen zwischen zwei Produktionsbuilds in Chromium, Firefox
  und WebKit ohne Wiederholung erfolgreich, einschließlich unterbrochener Updates
  und Schutz ungespeicherter Änderungen.
- Lighthouse 13.6.0: alle zehn mobilen Messungen und die unveränderten Assertions
  erfolgreich. Median für den öffentlichen Einstieg (`/` mit Weiterleitung zu
  `/login`): Performance 99/100, FCP 758 ms; für die angemeldete `/collection`:
  Performance 96/100, FCP 759 ms. Gefordert bleiben mindestens 90/100 und höchstens
  3.000 ms. Der erste Messlauf wurde durch eine parallel gestartete lokale
  npm-Neuinstallation unterbrochen; die abschließende Serie lief separat vollständig
  durch.
- Produktions- und Acceptance-Compose-Konfigurationen erfolgreich validiert.
- Vollständiger npm-Audit: keine Schwachstellen.
- RustSec: keine Sicherheitsmeldungen. Es bleiben Wartungshinweise für die bereits
  neuesten Versionen von `chrono` und `chrono-tz`
  ([RUSTSEC-2026-0543](https://rustsec.org/advisories/RUSTSEC-2026-0543.html),
  [RUSTSEC-2026-0555](https://rustsec.org/advisories/RUSTSEC-2026-0555.html)).
  Ein Ersatz durch eine andere Zeitbibliothek ist eine eigene Migration.

Die nachfolgenden Angaben dokumentieren die frühere Prüfung. Insbesondere deren
Image-CVE-Zahlen und Messergebnisse sind keine erneute Prüfung der aktuellen Images.

## Historische Prüfung vom 30. Oktober 2026

Die Versionen wurden gegen npm, crates.io, die GitHub-Releases der jeweiligen
Hersteller und die Container-Registries geprüft. Verwendet werden stabile Releases.
GitHub Actions sind mit vollständigen Commit-SHAs, externe Images mit den
Multiarch-Digests ihrer Versionstags fixiert. Die Pins müssen bei späteren
Sicherheitsupdates einschließlich neuer Builds desselben Versionstags erneuert werden.

### Versionsentscheidungen

| Bereich | Stand |
| --- | --- |
| Node.js / npm | 26.13.0 LTS / 12.2.1 |
| Rust | 1.99.1, lokal und in CI festgelegt; Slim-Trixie-Buildimage |
| Svelte / SvelteKit / Node-Adapter | 5.57.3 / 3.1.1 / 6.0.1 |
| Vite / Vitest / Playwright | 8.3.5 / 5.0.5 / 1.64.1 |
| Lighthouse / Lighthouse CI | 13.6.0 / 0.15.1 |
| MariaDB | 12.3.3 aus der aktuellen LTS-Linie |
| Caddy / Mailpit | 2.11.7 / 1.32.0 |

TypeScript bleibt auf 6.0.3: Die Peer-Abhängigkeiten von SvelteKit 3.1.1,
svelte-check 4.7.7 und typescript-eslint 8.72.1 unterstützen TypeScript 7.0.2
noch nicht gemeinsam. MariaDB bleibt auf der aktuellen LTS-Linie statt auf die
kurzlebigere 13.0-Linie oder einen Release Candidate zu wechseln.

Die SvelteKit-3-Migration umfasst die Vite-Konfiguration, `#lib`-Imports,
Navigation, reaktiven Seitenzustand, explizite private Laufzeitvariablen und
den Service Worker mit eigenem TypeScript-Projekt. Hinter Caddy wird das
Request-Protokoll über dessen bereinigten `X-Forwarded-Proto`-Header ermittelt;
der Node-Adapter unterstützt die bisherige `ORIGIN`-Variable nicht mehr.

Die aktualisierte Browserprüfung machte zwei Fehler sichtbar: Profilfelder konnten
durch einen zweiten, verspäteten Ladevorgang überschrieben werden; zugängliche Namen
der Heft- und Cover-Schaltflächen enthielten nicht alle sichtbaren Beschriftungen in
passender Reihenfolge. Beide Fehler sind behoben. Der Tauschtest identifiziert den
soeben abgeschlossenen Tausch jetzt anhand seiner ID, damit ältere Testläufe in der
Historie keine mehrdeutige Auswahl verursachen.

Die Anmelde-, Registrierungs- und Passwort-Reset-Formulare werden erst nach dem
clientseitigen Einhängen der Event-Handler aktiviert. Dadurch gehen schnelle
Eingaben vor Abschluss der Hydrierung nicht verloren und lösen keinen nativen
Formularversand aus.

### npm-Overrides

Lighthouse CI 0.15.1 pinnt noch ältere Werkzeuge. Gezielt aktualisiert werden:

- Lighthouse auf 13.6.0, einschließlich neuer Puppeteer-Abhängigkeiten.
- `tmp` auf 0.2.7 für Lighthouse CI und den indirekten Editor-Aufruf.
- `basic-ftp` unter `get-uri` auf 6.2.4.
- `uuid` unter Lighthouse CI auf 11.1.1, die korrigierte CommonJS-kompatible Linie.

Diese Overrides sollen entfallen, sobald Lighthouse CI passende Versionen selbst
verwendet. Die vorhandenen Lighthouse-Grenzwerte für Performance und FCP bleiben bestehen.
Der alte globale `cookie`-Override wurde entfernt, da er mit SvelteKit 3 inkompatibel ist.
Node-Adapter und Tailwind-Vite-Plugin sind reine Entwicklungsabhängigkeiten.

### Sicherheitsprüfungen und verbleibende Befunde

- RustSec: keine Sicherheitsmeldungen oder Warnungen in der aktualisierten `Cargo.lock`.
- `npm audit --omit=dev`: keine Befunde.
- Vollständiger npm-Audit: vier Meldungen niedriger Schwere, keine mittleren,
  hohen oder kritischen Meldungen. Ursache ist die ungepatchte indirekte
  Abhängigkeit `deepmerge` 4.3.1; npm zählt auch die betroffenen Elternpakete.
  Siehe [GHSA-gcqx-w48m-w2h4](https://github.com/advisories/GHSA-gcqx-w48m-w2h4).
- Die Security-CI prüft jetzt auch Entwicklungswerkzeuge und schlägt bei neuen
  hohen oder kritischen npm-Meldungen fehl.

Trivy 0.76.1 wurde mit aktueller Datenbank auf die gepinnten Images angewendet.
Scannerbefunde sind keine Aussage darüber, ob eine Schwachstelle im konkreten
Betrieb ausnutzbar ist. Auch aktuelle offizielle Images sind nicht frei von CVEs:

| Image | Hohe Befunde | Kritische Befunde | Einordnung |
| --- | ---: | ---: | --- |
| Caddy 2.11.7 Alpine | 9 | 1 | Alpine-Bibliotheken und in Caddy eingebundene Go-Abhängigkeiten |
| MariaDB 12.3.3 | 25 | 1 | Go-Laufzeit des Hilfsprogramms `/usr/local/bin/gosu` |
| Debian Trixie Slim | 43 | 0 | Für diese Debian-Pakete meldet der Scanner keine korrigierte Version |
| Gebautes Backend-Laufzeitimage | 47 | 0 | Debian-Pakete einschließlich Laufzeitbibliotheken; keine korrigierte Version gemeldet |
| Gebautes Frontend-Laufzeitimage | 0 | 0 | Paketmanager aus dem Laufzeitimage entfernt |
| Mailpit 1.32.0 | 0 | 0 | Kein hoher oder kritischer Befund |

Der kritische Caddy-Befund betrifft `brotli-libs`
([CVE-2026-111497](https://avd.aquasec.com/nvd/cve-2026-111497)); der kritische
MariaDB-Befund betrifft die in `gosu` eingebundene Go-Laufzeit
([CVE-2025-68121](https://avd.aquasec.com/nvd/cve-2025-68121)).
Einzelne Komponenten haben bereits Korrekturen, die verifizierten offiziellen
Images enthalten sie noch nicht vollständig. Die ebenfalls geprüfte
MariaDB-UBI10-Variante beseitigt die `gosu`-Befunde nicht.

Im Frontend-Laufzeitimage werden die unbenötigten Paketmanager npm und Yarn samt
ihren eingebundenen Abhängigkeiten entfernt. Für Rust wird das Slim-Buildimage
verwendet, um unter anderem die unbenötigten ImageMagick-Pakete des großen
Buildimages zu vermeiden. Das Backend-Laufzeitimage installiert Debian-Pakete
ohne zusätzliche empfohlene Pakete.

### Reproduzierbare Prüfung

```bash
cd frontend
npm ci
npm run lint
npm run format:check
npm run check
npx tsc --project src/service-worker/tsconfig.json --noEmit
npm run test:coverage
npm run build
npm audit --audit-level=high
npm audit --omit=dev
cd ..
cargo fmt --all --check
SQLX_OFFLINE=true cargo clippy --locked --workspace --all-targets -- -D warnings
SQLX_OFFLINE=true cargo test --locked --workspace
cargo audit
```

Die Rust-Integrationstests benötigen eine isolierte MariaDB-Testdatenbank über
`DATABASE_URL`. Browser- und Lighthouse-Prüfungen benötigen den E2E-Compose-Stack.
Die vollständigen Browser-Suites wurden wie in CI mit jeweils frischen Testdaten
ausgeführt. Der lokale Stack lief auf Port 18080, um eine Kollision mit Herd zu
vermeiden; die reguläre Compose-Konfiguration verwendet weiterhin Port 80.

### Prüfergebnisse

- 402 Rust-Tests erfolgreich, einschließlich der Datenbanktests mit MariaDB 12.3.3.
- 872 Frontend-Tests erfolgreich; Statement-Coverage 98,7 %.
- Rustfmt, Clippy mit `-D warnings`, ESLint, Prettier und Svelte-Typprüfung erfolgreich;
  der ausgelagerte Service Worker wurde zusätzlich separat mit TypeScript geprüft.
- Backend- und Frontend-Docker-Builds erfolgreich; alle Dienste im Test-Stack gesund.
- Chromium: 95 erfolgreiche Browserprüfungen, davon eine nach Wiederholung.
- Firefox: 95 erfolgreiche Browserprüfungen ohne Wiederholung.
- WebKit: 94 erfolgreiche Browserprüfungen, davon zwei nach Wiederholung.
  Ein bereits vorhandener Offline-Navigationstest wird wegen einer bekannten
  Einschränkung von Playwright WebKit übersprungen. Die Wiederholungen betrafen
  einen Tauschablauf, die Offline-Vorbereitung und das Passwort-Reset-Formular.
- Nach Ergänzung der Hydrierungsabsicherung auch beim Anfordern des Reset-Links:
  Passwort-Reset einschließlich E-Mail, Passwortwechsel und Ablehnung eines erneut
  verwendeten Tokens in allen drei Browsern ohne Wiederholung erfolgreich.
- Lighthouse 13.6.0: alle zehn mobilen Messungen und die unveränderten Assertions
  erfolgreich. Median für `/privacy`: Performance 94/100, FCP 757 ms;
  für die angemeldete `/collection`: Performance 100/100, FCP 758 ms.
  Gefordert bleiben mindestens 90/100 beziehungsweise höchstens 3000 ms.
