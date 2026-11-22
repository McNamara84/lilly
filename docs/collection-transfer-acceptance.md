# Abnahme: Editionen, Sammlungsübertragung und persönliche Datenauskunft

Stand: 22. November 2026. Dieser Bericht ordnet die Akzeptanzkriterien von
[#18](https://github.com/McNamara84/lilly/issues/18),
[#17](https://github.com/McNamara84/lilly/issues/17) und
[#70](https://github.com/McNamara84/lilly/issues/70) der Implementierung und den
ausgeführten Prüfungen zu. Die GitHub-Issues werden dadurch nicht verändert.

Die bestätigten Entscheidungen sind umgesetzt: dokumentiertes LILLY-Format mit
Vorlagen, ergänzender Import mit Konfliktbericht, privater asynchroner ZIP-Export
mit erneuter Anmeldebestätigung und 24 Stunden Downloadfrist sowie vollständige
sichtbare Unterhaltungen einschließlich empfangener Nachrichten.

## Nachweisquellen

- **Transfer-Integration:** [transfer_tests.rs](../backend/src/services/transfer_tests.rs),
  echte MariaDB, eigene Konten, temporäre private Dateien und reale Routerantworten.
- **Format:** [collection_transfer.rs](../backend/src/services/collection_transfer.rs),
  CSV/JSON-Parser, Normalisierung, Roundtrip, Dateiduplikate und Grenzwerte.
- **Archiv und Ablage:** [data_export.rs](../backend/src/services/data_export.rs)
  und [data_transfer.rs](../backend/src/services/data_transfer.rs).
- **Frontend:** [API-Tests](../frontend/tests/collection-transfer-api.test.ts),
  [Seitentests](../frontend/tests/collection-transfer-pages.test.ts),
  [Profiltests](../frontend/tests/profile-page.test.ts) und
  [Datenschutztests](../frontend/tests/privacy-page.test.ts).
- **Browser:** [Sammlungsübertragung](../frontend/e2e/collection-transfer.spec.ts)
  und [Datenauskunft](../frontend/e2e/data-export.spec.ts); echte Downloads,
  unabhängige ZIP-/SHA-256-Prüfung mit Python und automatische Axe-Prüfungen.
- **Regressionen:** [Sammlung](../frontend/e2e/collection.spec.ts),
  [Fotos](../frontend/e2e/photos.spec.ts), [Tausche](../frontend/e2e/trades.spec.ts),
  [API-Vertrag](../frontend/e2e/api-contract.spec.ts) und
  [Offlinebetrieb](../frontend/e2e/pwa-offline.spec.ts).

## #18: Editionen getrennt verwalten

Die vorhandene Exemplarfunktion wird weiterverwendet. Die neue dauerhafte
Exemplar-Kennung ergänzt die bestehende Kombination aus Nutzer, Heft und
Exemplarnummer; die Editionsbezeichnung ist weiterhin frei pflegbare Metadaten.

| Akzeptanzkriterium | Implementierung und ausgeführter Nachweis |
| --- | --- |
| Mehrere Einträge desselben Hefts | Bestehende Sammlungsanlage und Detaildialog; Transfer-Test `editions_keep_photos_status_notes_and_progress_independent` und Editionsablauf in `collection.spec.ts`. |
| Edition erkennbar | `edition_label` in Sammlung, Detaildialog und Transferformat; Browserablauf legt Erstauflage und Variantcover an und liest beide wieder aus. |
| Eigene Zustände, Status, Notizen und Fotos | Editions-Integrationstest speichert unterschiedliche Werte und echte Fotos, bearbeitet ein Exemplar und prüft das andere unverändert. |
| Unterscheidung in Sammlungsansichten | Bestehende Exemplarauswahl und Beschriftung; Editions-Browserablauf prüft Auswahl, Bearbeitung und Persistenz. |
| Fortschritt zählt Heftnummer einmal | Editions-Integrationstest und Fortschrittsregressionen prüfen mehrere Exemplare und Statuswechsel. |
| Wunsch- und Tauschansichten zeigen Edition | Bestehende Tauschprojektionen und Editions-Snapshots; reale Matching-/Tauschtests in `db/trade_matching.rs`, `trades.spec.ts` und Wunschlisten-Frontendtests. |
| Löschen erhält andere Editionen | Editions-Integrationstest und Editions-Browserablauf löschen ein Exemplar und prüfen verbleibende Metadaten/Fotos. |

## #17: Sammlung als CSV und JSON übertragen

| Akzeptanzkriterium | Implementierung und ausgeführter Nachweis |
| --- | --- |
| Vollständiger UTF-8-CSV-/JSON-Export | Kontogebundene Exportprojektion ohne Listenpagination oder Suchfilter; Format-Roundtrip und tatsächliche Browserdownloads beider Formate. |
| Stabile Heftreferenz, Serie, Nummer und persönliche Metadaten | Versioniertes Format mit Heftreferenz, optionaler Quellenreferenz und dauerhaftem `copy_key`; Roundtrip einschließlich Edition, Notiz, Status und Zustand. |
| Import beider Formate ins eigene Konto | Vorschau und explizite Bestätigung; Transfer-Test `imports_preserve_editions_repeat_across_formats_and_tombstone_deleted_copies`. |
| Fehlerhafte Zeilen, unbekannte Hefte, ungültiger Status/Zustand | Format- und Integrationsprüfungen unterscheiden Dateifehler, Zeilenfehler, Katalog-/Quellenfehler und Konflikte; Oberfläche zeigt Zähler und paginierte Ergebnisse. |
| Gültige Zeilen trotz anderer Fehler | Gemischte Integrationstests und Browserablauf mit zwei gültigen Editionen und unbekanntem Heft; nur gültige neue Exemplare entstehen. |
| Wiederholung erzeugt keine unbeabsichtigten Duplikate | Identische Dateien, Formatwechsel, vertauschte Reihenfolge, doppelte Zeilen, wiederholte Bestätigung, konkurrierende Importe und SQL-Fehler nach Teilfortschritt sind geprüft. Löschmarkierungen schützen bereits entfernte Kopien. |
| Kein fremdes Zielkonto und kein fremder Export | Nutzerkennung ausschließlich aus der aktiven Sitzung; manipulierte/fremde Aufträge und Adminzugriffe auf fremde Transfers liefern keine Daten. Unbekannte Kontrollfelder werden abgelehnt. |

Zusätzlich geprüft: leere Sammlung, BOM, Umlaute, Emoji, Semikolon,
Anführungszeichen, mehrzeilige Notizen, optionale Werte, Formelfelder und echte
Apostrophe. Die Vorschau verändert keine Sammlungseinträge. Der Worker prüft den
aktuellen Bestand erneut; keine Importaktion überschreibt vorhandene persönliche
Daten. Zeilenanlage und Ergebnisbeleg werden in derselben Transaktion gespeichert.

## #70: Vollständige persönliche Datenauskunft

| Akzeptanzkriterium | Implementierung und ausgeführter Nachweis |
| --- | --- |
| Eigenen Export anfordern | Geschützte API und Profilverknüpfung; reale Router-, Seiten- und Browserprüfungen. |
| Frische Anmeldebestätigung | Bestehender `RecentAuthUser`, Passwort und verknüpfte aktivierte OAuth-Provider; fehlende/veraltete Anmeldung, Passwortbestätigung und sicherer OAuth-Rücksprung geprüft. |
| Alle vorhandenen persönlichen Datenbereiche | Explizite Feldlisten in `data_export.rs`; repräsentative Fixtures für Konto, Sammlung, Einwilligungen, Provider, Sitzungen, Offline-Belege, Matching, Tausche, Nachrichten, Hinweise, Rollen-/Publikations-/Importaktivitäten und Transferbelege. |
| Hochgeladene Dateien | Echte eigene Fotos und Avatar im ZIP; erhaltene aktive Importquellen werden beigefügt. Fehlende erwartete Dateien verhindern ein als vollständig veröffentlichtes Archiv. |
| Geheimnisse und fremde private Daten ausgeschlossen | Zwei Nutzer und Admin, fremde Unterhaltungen, private Standort-/E-Mail-Felder und geheime Felder werden geprüft. Datenbankprojektionen verwenden ausdrückliche Feldlisten und gebundene Parameter. |
| Lesbares dokumentiertes Format | ZIP mit CSV, JSON, README und Manifest; [Formatbeschreibung](collection-transfer-format.md) und [Auskunftsbeschreibung](personal-data-export.md). Dateizahlen, Bytes und SHA-256 werden unabhängig verifiziert. |
| Zugriffsschutz und begrenzte Downloadfrist | Eigentümerbindung bei Erstellung, Status und Download; genau 86.400 Sekunden ab Fertigstellung, keine Verlängerung durch Downloads, frische Authentifizierung auch beim Download. |
| Abschluss/Fehler und befristete Dateien | Persistente Auftragszustände und UI; vollständige Veröffentlichung erst nach Archivprüfung. Exakte Ablaufgrenze, Schreib-/Größenfehler, Löschfehler mit Retry und Neustart/Restore werden geprüft. |
| Automatische Vollständigkeitsprüfung je vorhandenem Bereich | Transfer-Integrationstests `all_visible_messages_and_audit_data_export_without_other_accounts_private_fields`, `export_covers_session_offline_matching_and_catalogue_actor_metadata` und `personal_export_is_complete_private_includes_real_media_and_expires_without_extension`, ergänzt um Lösch-/Recoverytests. |

Der Nachrichtentest enthält 81 sichtbare Nachrichten einschließlich empfangener
Texte sowie eine ausgeschlossene fremde Unterhaltung. Kommentare und Bewertungen
sind noch nicht implementiert; das Manifest nennt beide ausdrücklich und enthält
leere Datensätze. Technische Sicherheitszustände und nicht personenbezogene
Katalogdetails sind im [Dateninventar](privacy-data-inventory.md) klassifiziert.

Eine eigene Kontolöschung sperrt Transfers sofort; eine Rücknahme aktiviert alte
Archive nicht. Partnerlöschung invalidiert betroffene Auskünfte. Die Löschphase
wartet auf die private Dateibereinigung. Neustart/Restore behandeln unterbrochene
Aufträge, fehlende Dateien und verwaiste Verzeichnisse; aktive Dateiproduzenten
sind gegen gleichzeitige Bereinigung geschützt.

## Ausgeführte Prüfungen und Messgrenzen

| Prüfung | Ergebnis |
| --- | --- |
| Rust, vollständiger Workspace mit echter MariaDB unter Linux | **472 Tests bestanden:** 362 Backend, 80 Adapter, 30 Core. |
| Zusätzliche native Transfer-Integration unter Windows | Alle 18 Transfer-Integrationstests bestanden. |
| Frontend, vollständige Vitest-Suite | **1.131 Tests in 76 Dateien bestanden.** |
| Neue Browserabläufe | **16 bestanden:** je vier Tests in Chromium, Firefox, WebKit und mobilem Chromium. |
| Bestehende Sammlung-/Foto-/Tausch-/Vertragsabläufe | **105 bestanden** über die drei Desktopbrowser. |
| Mobile Offline-Regressionsabläufe | **3 bestanden.** Damit insgesamt 124 Browserprüfungen. |
| Betriebs-/Konfigurationsskripte unter Linux | **179 Tests bestanden.** |
| Reale isolierte Betriebsabnahme | **7 Schritte bestanden:** Start, Registrierung/Verifikationsmail/Admin, Neustart mit Sammlung/Foto, Speicherfehler, Backup/Restore mit Ledger-Replay, Datenbankausfall/-erholung und Upgrade/Rollback. |
| Formatierung, Lint und Typen | Rustfmt, Clippy für alle Targets mit `-D warnings`, Prettier, ESLint und Svelte-/Service-Worker-Typprüfung bestanden; keine Svelte-Warnungen. |
| Build und Vertrag | Produktionsbuilds in Docker, Frontendbuild und OpenAPI-Aktualitätsprüfung bestanden. Vorhandene OpenAPI-Pfade und Schemas sind semantisch unverändert; hinzu kommen neun Pfade und acht Schemas. |

Gemessen wurde mit Vitest/V8 und LLVM (`cargo-llvm-cov` 0.9.1). Die Zahlen beziehen
sich auf die vollständigen Suiten, nicht nur auf erfolgreiche Beispielabläufe:

| Bereich | Abdeckung |
| --- | --- |
| Frontend gesamt | **98,80 % Statements**, 99,54 % Zeilen, 90,01 % Branches, 98,11 % Funktionen. |
| Neue Transfer-API und beide neuen Seiten | Jeweils **100 % Zeilen**; die Branch-Abdeckung bleibt darunter. |
| Rust-Workspace gesamt, Linux | **88,47 % Zeilen**, 86,31 % LLVM-Regions. |
| Neue Transfermodule: Persistenz, Routen, Format, Archiv, Worker | Zusammen **91,81 % Zeilen**; einzeln 95,79 %, 95,69 %, 95,44 %, 87,76 % und 88,26 %. |

Es wird keine vollständige Branch- oder Fehlerpfadabdeckung behauptet. Insbesondere
beliebige Datenbank-/Dateisystemfehler bei jedem einzelnen Systemaufruf,
Prozessbeendigung während jedes möglichen Await-Punkts und echte Providerstörungen
sind nicht vollständig automatisiert provoziert. Repräsentative Schreib-, Größen-,
Lösch-, Quellenverlust-, Datenbank-, Nebenläufigkeits- und Neustartfehler werden
gezielt geprüft. LLVM-Regions sind keine JavaScript-Statement-Metrik. Die native
Windows-LLVM-Messung erfasste Backendcode nicht zuverlässig; maßgeblich ist deshalb
der vollständige Linux-Lauf einschließlich der Unix-Dateiberechtigungstests.

Die zentralen Prüfungen sind reproduzierbar mit einer isolierten `DATABASE_URL`
und einer laufenden E2E-Umgebung:

```sh
# Repository root, SQLX_OFFLINE=true and isolated DATABASE_URL
cargo fmt --check --all
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo llvm-cov --locked --workspace --json --output-path .tmp/transfer-coverage.json -- --test-threads=1
python scripts/openapi.py --check
python -m unittest discover -s scripts/tests -p 'test_*.py'

# frontend/
npm run format:check
npm run lint
npm run check
npm run test:coverage
npm run build
npx playwright test --project=chromium --project=firefox --project=webkit --project=mobile-chrome e2e/collection-transfer.spec.ts e2e/data-export.spec.ts
npx playwright test --project=chromium --project=firefox --project=webkit e2e/collection.spec.ts e2e/photos.spec.ts e2e/trades.spec.ts e2e/api-contract.spec.ts
npx playwright test --project=mobile-chrome e2e/pwa-offline.spec.ts
```

Die reale Betriebsabnahme verwendet `scripts/operations_acceptance.py` mit den
aktuellen Kandidatenimages; ihr bereinigter lokaler Bericht liegt unter
`.tmp/transfer-operations-acceptance.json`. Coverage-Rohdaten und Laufprotokolle
bleiben ebenfalls in ignorierten lokalen Testartefakten.

Die Tests verwenden isolierte Datenbanken und Compose-Projekte sowie lokale
Katalog-/Medienfixtures. Es wurden keine produktiven Nutzerdaten verändert und
keine Live-Wikis oder echten OAuth-Anbieter benötigt. OAuth wird durch vorhandene
Protokolltests und neue UI-/Rücksprungtests abgesichert. Ein produktiver Rollout und
die Abnahme einer konkreten Serverinstallation sind nicht Teil dieser lokalen
Implementierungsabnahme.

Die Migration wurde in isolierten Datenbanken und frischen/neu gestarteten
Containern ausgeführt. Die Prüfung einer repräsentativen Kopie der tatsächlichen
Produktionsdaten bleibt ein Rolloutschritt gemäß [Server-Runbook](server-deployment.md).
