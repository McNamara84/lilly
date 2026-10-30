# Abhängigkeits- und Sicherheitsstand vom 30. Oktober 2026

Die Versionen wurden gegen npm, crates.io, die GitHub-Releases der jeweiligen
Hersteller und die Container-Registries geprüft. Verwendet werden stabile Releases.
GitHub Actions sind mit vollständigen Commit-SHAs, externe Images mit den
Multiarch-Digests ihrer Versionstags fixiert. Die Pins müssen bei späteren
Sicherheitsupdates einschließlich neuer Builds desselben Versionstags erneuert werden.

## Versionsentscheidungen

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

## npm-Overrides

Lighthouse CI 0.15.1 pinnt noch ältere Werkzeuge. Gezielt aktualisiert werden:

- Lighthouse auf 13.6.0, einschließlich neuer Puppeteer-Abhängigkeiten.
- `tmp` auf 0.2.7 für Lighthouse CI und den indirekten Editor-Aufruf.
- `basic-ftp` unter `get-uri` auf 6.2.4.
- `uuid` unter Lighthouse CI auf 11.1.1, die korrigierte CommonJS-kompatible Linie.

Diese Overrides sollen entfallen, sobald Lighthouse CI passende Versionen selbst
verwendet. Die vorhandenen Lighthouse-Grenzwerte für Performance und FCP bleiben bestehen.
Der alte globale `cookie`-Override wurde entfernt, da er mit SvelteKit 3 inkompatibel ist.
Node-Adapter und Tailwind-Vite-Plugin sind reine Entwicklungsabhängigkeiten.

## Sicherheitsprüfungen und verbleibende Befunde

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

## Reproduzierbare Prüfung

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

## Prüfergebnisse

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
