# MVP-Abnahme: Rollen, Startserien und gemeinsame API

Stand: 31. Oktober 2026. Umsetzung auf Basis von `ad3514b`, Branch
`feature/issues-19-44-74`. Die zugehörigen Änderungen und dieser Bericht gehören zum
gemeinsamen Abschlusscommit. Der separate Implementierungsplan wird nicht eingecheckt.

Die Nachweise für #44 und #74 sind erbracht. #19 bleibt wegen der unten dokumentierten
Quellenbefunde und des noch ausstehenden vollständigen John-Sinclair-Wiederholungsnachweises
offen. Ein erfolgreicher Teilnachweis wird nicht als vollständige MVP-Abnahme gewertet.

## #44 – User-/Admin-Rollen

| Akzeptanzkriterium | Nachweis |
| --- | --- |
| Baseline-Rollen User und Admin | Vorhandenes Rollenmodell und `AdminUser`-Guard; Datenbankrolle `user`/`admin`. |
| Neue Konten sind normale Nutzer | Passwort-/OAuth-Registrierung und deren Datenbanktests; Hochstufung über Bootstrap bzw. `admin promote`. |
| Import und Serienverwaltung nur für Admins | Alle registrierten Admin-Operationen werden aus dem API-Vertrag abgeleitet und gegen anonyme/gewöhnliche Nutzer geprüft. |
| Backend schützt jede administrative Aktion | Negative Tests erwarten `401`/`403`, einschließlich der zuvor nicht in der manuellen Matrix erfassten Kontolöschungs-Jobs. |
| Admin-Navigation für normale Nutzer verborgen | Bestehende Layout-/Navigationstests im Frontend. |
| Keine Selbsthochstufung | Profil-/Konto-DTOs bieten keine Rollenänderung; Registrierungstests prüfen die persistierte Standardrolle. |
| Sitzungspolitik dokumentiert | Access-Rolle gilt bis zum Ablauf; Refresh verwendet aktuelle Datenbankrolle. Standardlaufzeit 15 Minuten; Kontodeaktivierung widerruft Sitzungen zusätzlich. |
| Erlaubte und verbotene Zugriffe getestet | Rust-Routertests, bestehende Rollen-/Refresh-Tests und tatsächliche Admin-Importabläufe in Playwright. |

Die zentralen Nachweise stehen in `backend/src/openapi.rs`,
`backend/src/routes/admin.rs`, `backend/src/routes/auth.rs`,
`backend/src/services/admin_roles.rs` und `frontend/tests/admin-layout.test.ts`.

## #74 – Gemeinsame API

| Akzeptanzkriterium | Nachweis |
| --- | --- |
| Fachliche Operationen dokumentiert | 84 registrierte REST-Operationen in [openapi.json](openapi.json), einschließlich OAuth, Kontolöschung und Offline-Synchronisation. |
| PWA verwendet dieselbe API | Bestehende Clients unter `frontend/src/lib/api/`; Playwright prüft die tatsächlichen Fetch-Antworten gegen den gemeinsamen Vertrag. |
| Gemeinsame Backend-Validierung/Berechtigungen | Vorhandene Guards und Services bleiben maßgeblich; die neue Routerregistrierung erhält Middleware und Uploadgrenzen. |
| Eingaben, Antworten, Pagination, Fehler und Versionierung | [API-Handbuch](api.md), abgeleitete DTO-Schemas und einheitliche JSON-Fehler auch für Framework-Ablehnungen. |
| Rollen und Privatsphäre | Automatische Admin-Matrix sowie bestehende Integration-/E2E-Tests für Profile, Fotos, Sammlungen, Tausch und Nachrichten. |
| Verträge und Kompatibilität getestet | Offizielles OpenAPI-3.1-Schema, lokale Referenzprüfung, reale Response-Validierung, dokumentierte Änderungsregeln und CI-Exportvergleich. |
| API unabhängig von PWA nutzbar | Datenbankfreier CLI-Export; optionales, lokal ausgeliefertes Swagger UI mit Cookie-Sitzung und ohne CDN/externen Validator. |
| Repräsentative Ende-zu-Ende-Abläufe | Sammlung, Import/Freigabe, Tausch/Nachrichten, Profil und Medien laufen durch denselben Vertragsvalidator. |

Die Diagnose des WebKit-Gesamtlaufs deckte außerdem einen Testfehler auf: Die
IndexedDB-Bereitschaftsprüfung konnte vor der Anwendung eine leere Datenbank anlegen.
Die Prüfung bricht nun einen solchen eigenen Upgradeversuch ab und wartet auf die
von der Anwendung erstellten Object Stores. Das Produktverhalten wurde dafür nicht geändert.

## #19 – Vollständige Startserien

| Akzeptanzkriterium | Nachweis |
| --- | --- |
| Alle verfügbaren Hefte beider Startserien | Maddrax: 701 Quellhefte. Korrigierter John-Sinclair-Adapter: 2.522 Quellhefte ohne Lücken; vollständiger Persistenznachweis nach Behebung der Quellenbefunde noch offen. |
| Pflichtmetadaten und Erscheinungsdatum | Serienreferenz, Heftnummer, Titel, Autoren und Provenienz werden geprüft. Die bestätigte Anforderungsänderung lässt unbekannte Daten zu und validiert vorhandene Daten. |
| Eindeutige Serie/Heftnummer | Bestehende Datenbank-Constraints, Persistenztests und unabhängige Prüfung aller importierten Nummern, IDs und Quellenidentitäten. |
| Fehlende/fehlerhafte Pflichtmetadaten werden gemeldet | Parser-/Vertragstests, persistierte Prüfbefunde und blockierte Freigabe; der tatsächliche Datumsfehler bei John Sinclair 191 wurde so erkannt. |
| Wiederholungen aktualisieren ohne Duplikate | Datenbanktest mit unveränderten Heft-IDs und Sammlungsdaten; verschärfter Live-Vergleich einschließlich Metadaten-Hash und Ergebniszählern. John-Sinclair-Nachweis bleibt offen. |
| Automatische Integrationstests für sechs Referenzen | `built_in_reference_records_round_trip_through_persistence_idempotently` sowie lokale Adapter-Fixtures und Vertragsprüfungen im regulären Workspace-Testlauf. |

Die bestätigte Datumsregel ist in Anforderungen, Importvertrag, Adapterhandbuch und
README vereinheitlicht: Das Erscheinungsdatum ist optional. Bekannte vollständige Daten
werden validiert; leere bzw. ausdrücklich unbekannte Werte bleiben optional und werden
später ergänzt. Die echten Quellen verwenden unter anderem `??????` als Unbekannt-Marker.
Ein Regressionstest prüft auch den Rückgriff auf ein bekanntes Übersichtsdatum.

Ein Datenbanktest weist `Datum fehlt → später bekanntes Datum` bei stabiler Heft-ID und
unveränderten Sammlungsdaten nach. Alle sechs fest vorgegebenen Referenzhefte bleiben in
den automatischen Parser-/Persistenztests enthalten.

Die Live-Abnahme verwendet das isolierte Compose-Projekt `lilly-mvp-acceptance` mit eigenen
Volumes und Loopback-Ports 18080/18025. Demo-Seeding, E2E-Adapter und Scheduler sind aus.
Die reproduzierbaren Schritte und das lesende Prüfskript sind in
[Lokale Vollimport-Abnahme](mvp-import-acceptance.md) beschrieben.

### Befunde aus echten Quellen

- Maddrax, erster vollständiger Lauf (Job 1): 701 entdeckte Hefte, davon 699 importiert
  und zwei zukünftig erscheinende Hefte übersprungen. Keine Fehler/Warnungen, keine
  Nummernlücken, alle 699 Cover vorhanden, alle drei Referenzen bestanden. Anschließend
  lokal freigegeben und ein Sammlungsdatensatz mit Zustand und Notiz angelegt. Übersprungen
  wurden Nr. 700 „Blackout“ (Quelldatum `2026-11-14`) und Nr. 701 „Kampf um das Licht“
  (`2026-11-28`).
- Die ersten John-Sinclair-Läufe wurden nach reproduzierbaren Quellbefunden geordnet
  abgebrochen. Das Wiki lieferte wiederholt HTTP 500 bzw. `DBConnectionError` im JSON.
  Diese temporären Antworten werden nun begrenzt wiederholt, auch bei Covern. Die Pausen
  steigen auf maximal vier Sekunden; höchstens acht Versuche. Nicht transiente Fehler
  werden weiterhin unmittelbar gemeldet, vorhandene Cover bei Fehlern erhalten.
- Der nächste John-Sinclair-Lauf erkannte das ungültige Quelldatum `29.02.1982` bei
  Heft 191. Der Projektverantwortliche korrigierte den Eintrag direkt im Wiki auf
  `28.02.1982`. Der Importer enthält dafür keine Sonderregel; die normale Datumsprüfung
  bleibt erhalten. Der anschließende Vollscan speicherte Heft 191 mit `1982-02-28`.
- Der Maddrax-Wiederholungslauf zeigte 146 als aktualisiert gezählte Hefte. Ursachen
  waren unterschiedliche Sortierreihenfolgen in Rust/MariaDB sowie durch die bisherige
  Eindeutigkeitsprüfung zusammengefasste Groß-/Kleinschreibung, Akzente und Leerzeichenvarianten. Der Vergleich
  ignoriert nun Listenreihenfolgen; eine Migration erhält die exakten Quellschreibweisen.
  Datenbanktests prüfen den unveränderten Wiederholungslauf, echte Schreibweisenänderungen,
  stabile Heft-/Sammlungs-IDs und weiterhin von Groß-/Kleinschreibung und Akzenten
  unabhängige Autorensuche in Sammlung, Tauschlisten und Suchkandidaten. Der erweiterte
  Datenbanktest für Tauschlisten schlug vor der SQL-Korrektur gezielt fehl. Das Abnahmeskript
  vergleicht zusätzlich einen Metadaten-/Cover-Hash und verlangt beim unveränderten
  Wiederholungslauf `created = 0` und `updated = 0`.
- Maddrax nach Anwendung der Schreibweisen-Migration (Job 6): 701 vollständig verarbeitete
  Quellhefte, 36 korrigierte und 663 unveränderte Datensätze, zwei zukünftige Hefte
  übersprungen. Keine Fehler/Warnungen; 699 Hefte mit Cover, keine fehlenden Daten,
  Nummernlücken oder Referenzabweichungen. Heft-Identitäten und Sammlungs-Hash stimmen
  mit dem ursprünglichen vollständigen Import überein.
- John Sinclair 571 und 1776: Die verlinkten Quelldateien `Datei:JS 0571.jpg` und
  `Datei:JS 1776.jpg` werden von der Wiki-API
  ausdrücklich mit `missing: true` und ohne `imageinfo` gemeldet. Die bibliografischen
  Metadaten werden importiert; die fehlenden Coverabrufe bleiben als überprüfte Warnungen
  sichtbar. Ein Ersatzbild wird nicht aus einer anderen Ausgabe abgeleitet. Die
  gesonderten Cover-Issues #22/#46 werden durch diese Abnahme nicht geschlossen.
- John Sinclair 2264: Auch nach acht Versuchen meldete die Quelle HTTP 500. Die
  anschließende Diagnose bestätigte Datenbankverbindungsfehler des Wikis sowohl über
  `action=parse` als auch über den direkten Quelltextabruf; die Revisions-API lieferte
  ebenfalls HTTP 500. Ein Wechsel des Transportwegs würde diesen Befund nicht beheben.
- John Sinclair 2315: Die Übersicht verlinkt `JS 2315 - Die Visonäre`, das die API als
  fehlend meldet. Die korrekt geschriebene Seite `JS 2315 - Die Visionäre` existiert
  (Wiki-Seiten-ID 7551). Dieser Quellenfehler bleibt ein blockierender Befund, bis die
  Verknüpfung korrigiert und ein vollständiger Import erfolgreich geprüft wurde.
- Der vollständige Diagnoselauf (Job 7) endete nach 2.517 erfassten Quellzeilen mit
  2.514 gespeicherten Heften, 2.512 Covern, 499 unbekannten Erscheinungsdaten, zwei
  blockierenden Abrufbefunden und zwei Coverwarnungen. Alle drei Referenzhefte bestanden.
  Heft 2522 wurde wegen des Quelldatums `2026-11-07` übersprungen. Die Serie blieb inaktiv.
- Der unabhängige Nummernvergleich fand fünf vom bisherigen Übersichtsparser übergangene
  Zeilen: 204, 1992, 2005, 2006 und 2041. Ihnen fehlen optionale nachfolgende Tabellenspalten.
  Der korrigierte Parser erfasst diese Zeilen; echte Quell-Fixtures und Tests für jede
  Spaltengrenze sichern das Verhalten ab. Ein lesender Live-Abruf mit dem korrigierten
  Adapter ergab **2.522 Quellhefte ohne Nummernlücken** und gültige Detaildaten für alle
  fünf Hefte. Das unabhängige Abnahmeskript lehnt ungeklärte Nummernlücken nun ausdrücklich
  ab. Der vollständige Persistenz- und Wiederholungsnachweis für John Sinclair muss nach
  Klärung der Quellenbefunde mit diesem Parser erneut ausgeführt werden.

Der abschließende Maddrax-Wiederholungslauf (Job 8, `09:58:02–10:17:26 UTC`) bestand:
701 Quellhefte verarbeitet, 699 unverändert, zwei zukünftige Hefte übersprungen,
keine Neuanlagen, Aktualisierungen, Fehler oder Warnungen. Identitäts-, Metadaten-/Cover-
und Sammlungs-Hashes stimmen mit dem reparierten Ausgangsstand aus Job 6 überein.
Heft-IDs und Nutzerdaten stimmen außerdem mit dem ursprünglichen Import aus Job 1
überein. Alle drei Referenzen und der öffentliche Katalog wurden unabhängig geprüft.

Für John Sinclair stehen nach der bestätigten Wiki-Korrektur noch der vollständige
Import mit dem korrigierten Parser, die Freigabe und der Wiederholungsnachweis aus.
Bis zu deren erfolgreichem Abschluss bleibt #19 offen.

### Vorbereitete Anpassung für Issue #19

Die Issue-Beschreibung und das zweite Akzeptanzkriterium sollen gemäß bestätigter
Entscheidung lauten:

> Every imported issue contains a series reference, numeric issue number, title, at least
> one author, and complete source provenance. The first publication date is optional;
> supplied dates are validated, and unknown dates are backfilled by subsequent imports.

Die sechs konkreten Referenzdaten und die übrigen Kriterien bleiben bestehen. Die
GitHub-Issues werden erst nach Integration in `main` mit den jeweils vollständigen
Nachweisen geschlossen.

## Automatische Prüfungen

Die Prüfungen verwenden Rust 1.99.1, Node 26.13.0, npm 12.2.1 und eine separate MariaDB
12.3.3. Live-Wikis gehören nicht zur regulären CI-Testsuite.

| Prüfung | Ergebnis |
| --- | --- |
| Rust-Workspace mit Datenbank | 420 Tests (311 Backend, 79 Adapter, 30 Core); Coverage-Lauf mit seriellen Datenbanktests bestanden. |
| Rust-Coverage (`cargo llvm-cov`) | 87,55 % Zeilen, 86,44 % Regionen im Workspace. Neue Routerregistrierung, Datumshilfe und MediaWiki-Fehlerbehandlung: 100 % Zeilen; OpenAPI-Modul: 99,53 %. |
| Frontend/Vitest | 885 Tests bestanden; 98,72 % Statements, 88,67 % Branches. Neuer Vertragsvalidator: 100 % Zeilen, Funktionen und Branches. |
| Abnahme-Prüfskript | 13 Tests einschließlich Fehlerfällen, Pagination, Nummernlücken trotz übereinstimmender Zähler, Metadaten-/Wiederholungsvergleich, CLI und Geheimnisbereinigung. |
| Chromium / Firefox | Je 97 E2E-Tests bestanden. |
| WebKit | 96 E2E-Tests bestanden; ein bestehender browserspezifischer Skip. |
| Nachprüfung nach Datenbankmigration | 36 Chromium-Tests für Sammlung, Admin-Import und API-Vertrag bestanden; nach Ergänzung der Tausch-Suchabfragen zusätzlich alle drei Tausch-E2E-Tests. |
| Statische Prüfungen | Clippy mit `-D warnings`, Rustfmt, ESLint, Prettier und Svelte-Check erfolgreich; OpenAPI-Export stimmt mit Code überein. |
| Abhängigkeiten | RustSec ohne Schwachstellen/Warnungen. npm-Audit ohne hohe/kritische Funde; vorhandene niedrige/mittlere Tooling-Funde bleiben sichtbar. |

Wesentliche Befehle:

```sh
cargo test --locked --workspace
cargo fmt --check --all
cargo clippy --locked --workspace --all-targets -- -D warnings
python scripts/openapi.py --check
python -m unittest discover -s scripts/tests -p 'test_*.py'
```

Frontend: `npm ci`, `npm run lint`, `npm run format:check`, `npm run check`,
`npm run test:coverage` und die drei Playwright-Projekte. Lokal wurde die Unit-Coverage
mit vier parallelen Workern gemessen, um gleichzeitige Rust-/Docker-Builds nicht zu
überlasten. Für getrennte lokale Ports unterstützen die Browsertests
`PLAYWRIGHT_BASE_URL` und das vorhandene `MAILPIT_API_URL`. Vor jedem Browser-Gesamtlauf
wird der isolierte E2E-Backend-Prozess neu gestartet, damit vorherige Testanmeldungen
keine Login-Ratenlimits für den nächsten Lauf aufbrauchen.
