# Abnahme: PWA und Performance (#111, #56, #58)

Stand: 31.10.2026. Dieser Bericht gehört zu dem Commit, der ihn einführt. Die lokalen
Messungen verwenden die Git-Basis `28adf4b` mit den Änderungen dieses Commits; der
Messrunner kennzeichnet den noch nicht committeten Stand ausdrücklich. Der
Implementierungsplan ist kein Bestandteil des Commits.

Die vereinbarte Abdeckung umfasst Chromium, Firefox und WebKit automatisiert.
Echte Android-/iOS-Geräteprüfungen sind ausdrücklich auf später verschoben.
Ein erfolgreicher Abschluss dieses Pakets erklärt nicht das gesamte MVP für fertig.

## Funktionale Nachweise

| Kriterium | Implementierung und Nachweis | Ergebnis |
| --- | --- | --- |
| Installation und Offline-Einstieg | Manifest, Icons, Installationsereignisse; `pwa-offline.spec.ts`, `pwa-lifecycle.test.ts` | Automatisiert; Gerätetests offen |
| Beide Startserien offline | Deterministischer Katalog mit je 100 Heften; `pwa-resilience.spec.ts` | Browserprüfung |
| Dauerhafte Änderungen | Atomare Transaktion für Eintrag und Queue; persistierte temporäre IDs und Reihenfolge; `offline-queue-resilience.test.ts` | Einschließlich Transaktionsabbruch |
| Große Queue | 0/1/100/101 Mutationen, höchstens 100 pro Anfrage; Fehler im zweiten Paket, Wiederaufnahme | Unit- und Backend-/Browsernachweis |
| Verlorene Antwort | Identische UUID und Payload beim Wiederholen; Browser lässt den Server verarbeiten und verwirft die erste Antwort | Keine doppelte Anlage |
| Gleichzeitiges Bearbeiten | Versuchte Payload unveränderlich; Folgeänderung erhält eigene UUID und später Server-ID/Revision | Unit- und Integrationstests |
| Mehrere Tabs | Atomare Sperre pro Benutzer, 60 Sekunden gültig, alle 20 Sekunden verlängert | Übernahme nach Ablauf; A/B-Test mit zweitem Tab |
| Konfliktentscheidung | Serverstand übernehmen oder aktuellen lokalen Wunsch mit neuer UUID erneut anwenden | Beide Wege in drei Browsern |
| Snapshot trotz ausstehender Änderungen | Lokaler Inhalt und Katalogreferenzen bleiben erhalten | Regressionstests für Snapshot und Listenantwort |
| Logout und Kontowechsel | Bestätigung bei Datenverlust, sofortige Sperre/Broadcast, Sitzungsversion gegen verspätete Antworten | Unit-Tests; Logout in zwei Tabs, Anmeldung als anderer Nutzer im selben Browser, ausschließlich dessen Daten offline |
| Fehler beim lokalen Löschen | Sichtbare Fehlermeldung; gesperrte Identität wird weder offline noch über alte Cookies reaktiviert; abgebrochene Bereinigung beim Sitzungsstart wiederholen | Fehlerpfad und physische Datenlöschung beim Kontowechsel getestet; ausdrücklicher Login möglich |
| Private Daten | API-Antworten und persönliche Fotos nicht im gemeinsamen Cache; benutzergebundene IndexedDB-Daten | Cache-Policy-/Service-Worker-Tests und Browser-Logout |
| Produktionsupdate A → B | Zwei tatsächlich gebaute Versionen unter einer Origin; Queue, zweiter Tab mit Eingaben, alte Assets, Cachebereinigung | 6 Tests bestanden, je 2 pro Browser |
| Unterbrochener Download von B | A bleibt verwendbar, B erneut installierbar; Aktivierungsfehler sichtbar | Drei Browser und Unit-Fehlerpfad |
| Onlinepflichtige Funktionen | Tausch, Nachrichten, OAuth und Fotos; direkte Offline-Navigation mit Hinweisseite | Bestehende Browser-/Komponententests |
| Parallele Sammlungsvorgänge | Sperren vor dem ersten Snapshot; vorübergehende MariaDB-Konflikte bei Sync, Anlegen und Löschen höchstens fünfmal mit wachsender Wartezeit versuchen | MariaDB-Integration mit 16 parallelen Anlagen und Löschungen |

Das lokale Schema bleibt Version 1: zusätzliche Queue-Felder sind optional, bestehende
Datensätze werden weiter gelesen. Es werden keine Benutzerdaten für ein Update gelöscht.
Nach einem abrupt beendeten Tab kann die Wiederaufnahme bis zum Ablauf seiner
60-Sekunden-Sperre warten; die Queue bleibt währenddessen sichtbar und gespeichert.

## Testumgebung und Ausführung

- Windows, Node 26.13.0, npm 12.2.1, Playwright 1.64.1.
- Chromium 156.0.8078.4 (Revision 1248), Firefox 157.0 (1555), WebKit 27.2 (2370).
- Eigener Compose-Projektname `lilly-pwa-e2e`, App-Port 18082, Mailpit 18027;
  separate MariaDB-Testdatenbank für Rust. Keine vorhandenen Abnahmevolumes verwendet.
- Der Demo-Seed aktiviert PWA-Testkonten und Cover ausschließlich bei
  `ENABLE_DEMO_SEED=true` und positiver `E2E_WORKER_COUNT`.
- Die E2E-Compose-Datei erhöht Anfragelimits für die vielen über dieselbe Proxy-Adresse
  laufenden Testnutzer. Produktionslimits bleiben unverändert.
- `npm ci --offline` erfolgreich; Lockdatei konsistent, keine neue Abhängigkeit.
- Frontend: 917 Tests bestanden; Statements 97,80 %, Branches 87,86 %,
  Funktionen 96,79 %, Zeilen 98,86 %.
- Rust: 319 Backend-, 80 Adapter- und 30 Core-Tests mit echter Datenbank bestanden,
  einschließlich des zusätzlichen Paralleltests.
- Lint, Typprüfung, Formatierung, Rustfmt, Clippy und Produktionsbuild erfolgreich.
- Abschließende vollständige Browserregression: **311 bestanden** in 5,6 Minuten,
  sieben erwartete Ausnahmen (sechs separate A/B-Fälle und eine WebKit-Offline-Navigation).
  Alle 15 Barrierefreiheitsfälle sind enthalten; die zuvor gefundenen Kontrastfehler
  sind behoben.
- Abschließende mobile Chromium-Emulation: **10 von 10 bestanden**.
- In diesen Browserläufen enthalten sind die PWA-Prüfungen:
  **39 bestanden, eine dokumentierte WebKit-Ausnahme**, verteilt auf Chromium,
  Firefox, WebKit und mobile Chromium-Emulation. Der Kontowechsel prüft beide Konten
  ohne Zurücksetzen des Browserprofils; der neue große Snapshot erhält 15 Sekunden
  für seinen Erstaufbau, der vollständige Ablauf höchstens 60 Sekunden.

Reproduktionsbefehle (Frontend-Verzeichnis, isolierter Stack läuft):

```sh
npm run test:coverage
npm run test:e2e:all
npx playwright test --project=mobile-chrome e2e/pwa-offline.spec.ts e2e/pwa-resilience.spec.ts
npm run test:pwa:update
npm run test:lighthouse
```

`test:pwa:update` startet einen nur lokal erreichbaren Testproxy und zwei Node-Server,
schaltet per zufälligem Testtoken zwischen Builds um und beendet die eigenen Prozesse
anschließend. Die sechs Updatefälle sind im normalen Einzelbuild-Durchlauf bewusst
ausgelassen und werden im verpflichtenden separaten CI-Schritt ausgeführt.

## Performance-Nachweis

Verbindlich sind fünf kalte Läufe je Strecke: anonymer Einstieg `/` bis zur sichtbaren
Anmeldung und authentifizierte `/collection` mit tatsächlich sichtbarer Sammlung und
geladenem Cover. Jeder Lauf verwendet einen neuen Lighthouse- und Browserprozess
mit genau einem Messtab; nur im zweiten Fall
werden Cookies aus einem direkten API-Login gesetzt. Service Worker, Cache Storage,
IndexedDB und HTTP-Cache sind vor der Navigation leer.

Fixture: 200 Einträge, je 100 aus Maddrax und John Sinclair; Z0 bis Z4;
120 eigene, 40 doppelte und 40 gesuchte Exemplare. 200 unterschiedliche lokale JPEGs
mit 320 × 480 Pixeln verhindern Abhängigkeiten von externen Wiki-Servern.
Diese kontrollierte Mindestgröße ersetzt keine Messung sehr großer realer Kataloge.

Profil: 412 × 823 CSS-Pixel, DPR 1,75; simulierte mobile Drosselung mit RTT 150 ms,
Durchsatz 1.638,4 Kbit/s, Request-Latenz 562,5 ms, Download 1.474,56 Kbit/s,
Upload 675 Kbit/s und CPU-Faktor 4. Lighthouse 13.6.0 mit der von Playwright installierten
Chromium Headless Shell 156.0.8078.4. Der Runner ermittelt deren plattformspezifischen
Pfad aus Playwrights Browserregister; `CHROME_PATH` erlaubt eine ausdrückliche Alternative.
Browservariante und tatsächliche Versionsmeldung stehen im Ergebnis-JSON. Grenzwerte gelten pro Strecke:
FCP-Median ≤ 3.000 ms, Performance-Median ≥ 90. Ungültige Navigationen, falsche
Identität, fehlende Inhalte oder Messwerte sowie Browserfehler lassen den Lauf scheitern.

Die abschließende Serie hat **beide Gates bestanden**. FCP-Werte in dieser Tabelle
sind auf ganze Millisekunden gerundet; die unveränderten Messwerte, Toolversionen
und das Profil stehen in [pwa-performance-results.json](pwa-performance-results.json).

| Strecke | FCP der fünf Läufe (ms) | Performance der fünf Läufe | FCP-Median | Performance-Median |
| --- | --- | --- | --- | --- |
| Anonymer Einstieg `/` → `/login` | 764 / 769 / 760 / 770 / 757 | 99 / 99 / 99 / 99 / 99 | **764 ms** | **99** |
| Angemeldete Sammlung `/collection` | 761 / 796 / 761 / 784 / 764 | 98 / 96 / 96 / 98 / 98 | **764 ms** | **98** |

Alle zehn Läufe waren gültig und wurden vollständig berücksichtigt. Rohberichte
liegen lokal in `frontend/.lighthouseci/`; CI archiviert sie privat für 14 Tage und
schlägt bei fehlenden Artefakten fehl. Cookies werden vor dem Speichern geschwärzt.

Mehrere Messserien mit dem vollständigen Chromium-Browser wurden wegen `NO_NAVSTART`
vollständig verworfen. Die aufgezeichnete Fehlerspur enthielt Navigationsereignisse ohne
Dokument-URL; Lighthouse konnte daraus keinen gültigen Messbeginn bestimmen, obwohl
die Anmeldung sichtbar war. Prozessisolation, das Schließen zusätzlicher Tabs und das
Weglassen eines zusätzlichen Leer-Seiten-Aufrufs beseitigten diesen Aufnahmefehler nicht.
Die bereits installierte Headless Shell derselben Chromium-Version lieferte anschließend
acht gültige Diagnoseläufe hintereinander und wird nun ausdrücklich als Messbrowser
verwendet. Diese Diagnoseläufe gehen nicht in die Abnahmemediane ein.

Die erste gültige vollständige Messserie erreichte für die Anmeldung 99 Punkte,
für die Sammlung jedoch nur 87 Punkte im Median. Das erste sichtbare Cover wurde
noch verzögert geladen; es erhält jetzt `loading="eager"` und `fetchpriority="high"`,
während weitere Cover weiterhin verzögert laden. Außerdem fehlte im Standard-Caddyfile
die bereits im Deployment konfigurierte Komprimierung. `encode zstd gzip` gilt nun
auch für den lokalen und CI-Stack, insbesondere für den großen Offline-Snapshot.
Die Grenzwerte und das mobile Messprofil wurden dafür nicht verändert.

Der Runner verwendet pro Messung einen eigenen Prozess und genau einen Tab.
Auch ungültige JSON-/HTML-Berichte werden vor der Validierung gespeichert und Cookies
darin geschwärzt; diese Regeln sind getestet. Die abschließende vollständige Abnahmeserie lief
ohne parallele Build- oder Browsertests. Es werden keine besten Einzelwerte aus mehreren
Serien ausgewählt und keine fehlerhaften Läufe stillschweigend wiederholt.

Die vollständigen Regressionstests führten außerdem zu Korrekturen an Testdaten und
Implementierung: Serientests wählen Maddrax ausdrücklich statt nach Listenposition;
Queue-Fixtures verwenden getrennte Heftnummern pro Worker. Beim Anlegen werden Sperren
vor dem ersten konsistenten Lesen gesetzt, damit wartende Transaktionen keinen veralteten
Stand für die Exemplarvergabe behalten. Numerische MariaDB-Codes werden getrennt vom
SQLSTATE ausgewertet. Ein wiederholter kompletter Snapshot-Austausch blockierte in
WebKit nachfolgende Datenbankzugriffe rund sechs Sekunden. Der Abgleich schreibt jetzt
nur geänderte Datensätze und löscht entfernte Datensätze; wiederholte Snapshots,
Katalogkorrekturen, entfernte Einträge und erhaltene lokale Änderungen sind getestet.
Die beiden betroffenen Browserabläufe bestanden danach jeweils zweimal.
IndexedDB-Löschungen werden innerhalb der Transaktion gebündelt. Verlorene Antworten werden direkt
an der Fetch-Grenze nach echter Serververarbeitung simuliert, weil WebKit solche
Service-Worker-Anfragen nicht zuverlässig durch `page.route` leitet.

Ein Sync-Timeout erhält ebenfalls die vorgemerkte Änderung und dieselbe Mutations-ID;
das Formular behandelt den lokal gespeicherten Vorgang als ausstehend. Die vollständige
Regression deckte außerdem einen bestehenden Kontrastfehler der Benachrichtigungszahl
auf: Weiß auf Rot erreichte nur 3,76:1. Schwarzer Text auf derselben Hintergrundfarbe
behebt die vier betroffenen Barrierefreiheitsbefunde.

Der zusätzliche Kontowechseltest fand unter WebKit eine bei schneller Navigation
abgebrochene lokale Löschung. Der nächste Sitzungsstart bereinigt deshalb verbleibende
gesperrte Profile, bevor er ein anderes Konto bestätigt. Doppelt oder verspätet
eintreffende Logout-Nachrichten erzeugen keine zweite Sitzungsversion, die eine neue
Anmeldung abbrechen könnte. Der Test prüft die tatsächlich gespeicherten Benutzer-IDs
und anschließend die Offline-Anzeige des neuen Kontos.

## CI-Nachprüfung vom 01.11.2026

Der [CI-Lauf 159](https://github.com/McNamara84/lilly/actions/runs/41272229978)
zu `f693ffc` fand einen Fehler, den die ursprüngliche lokale Browserabnahme nicht
ausgelöst hatte: Beim Annehmen und anschließenden Aufräumen eines Tauschs scheiterten
Transaktionen mit MariaDB-Deadlock `1213 / 40001`. Die Sammlung wiederholte solche
Transaktionen bereits, die Tauschabläufe noch nicht. Der verbliebene offene Tausch
blockierte anschließend die Testwiederholungen.

Vorschlag, Annahme, Abbruch und Abschluss wiederholen nun ausschließlich vollständig
zurückgerollte Transaktionen bei vorübergehenden Datenbankkonflikten, mit insgesamt
höchstens fünf Versuchen.
Die Fehlerklassifikation wird mit der Sammlung geteilt. Antwortabfragen nach dem Commit
und externe Fotooperationen liegen außerhalb der Wiederholung. Fachliche Konflikte,
fehlende Berechtigungen und Verbindungsfehler werden nicht pauschal wiederholt.

Ein MariaDB-Regressionstest injiziert den Fehler nach bereits erfolgten Schreibvorgängen.
Er scheiterte vor der Korrektur mit demselben Fehlercode und besteht danach einschließlich
Wiederholungsgrenze, Rollback, einmaliger Benachrichtigungen und unveränderter
Tausch-/Sammlungskonsistenz. Die PWA-Fixture erzeugt ihre Bilder vor dem Öffnen der
Datenbanktransaktion; auch deren Datenbankteil kann begrenzt wiederholt werden.
Tests mit gemeinsamen globalen Katalogabgleichen verwenden die vorhandene Testsperre;
beabsichtigte Parallelität innerhalb eines Tests bleibt erhalten.

Der WebKit-Job endete separat mit einem Runner-Shutdown und Exitcode 143. Sein Log
enthält keinen vollständigen Fehlerbericht; daraus lässt sich keine konkrete
Anwendungsursache ableiten. CI protokolliert jetzt den Namen jedes gestarteten Tests,
damit ein abgebrochener Lauf mehr Informationen hinterlässt. Firefox, Frontend-/Backend-
Prüfungen und Security waren im ursprünglichen GitHub-Lauf bereits erfolgreich.

Die zusätzliche Linux-Prüfung reproduzierte einen WebKit-Hänger in der Wunschliste:
50 Kandidatenkarten mit je einem Hintergrund-Weichzeichner ließen den Speicherbedarf
des isolierten Browser-Testcontainers auf über 7 GiB wachsen; bereits das Anklicken einer Checkbox
hing. Ein kontrollierter Vergleich ohne diese Filter bestand denselben Ablauf dreimal
in 25,1 Sekunden. Die Karten verwenden deshalb auf dem flachen Listenhintergrund keinen
eigenen Hintergrundfilter mehr; Farben, Rahmen und Schatten bleiben erhalten. Der
Browsertest prüft ausdrücklich die Bedienbarkeit einer vollen Seite mit 50 Karten.
Ein Fehler beim Aufräumen eines Tests verdeckt außerdem nicht länger den ursprünglichen
Browserfehler. Ob dieser Speicherbedarf den GitHub-Runner-Shutdown verursacht hat,
bleibt ohne vollständige Runnerdiagnose unbewiesen.

Bei der lokalen Nachprüfung bestehen alle 917 Frontend- und 432 Rust-Tests; die
Datenbanktests verwenden echte MariaDB. Formatierung, Typprüfung, ESLint, Rustfmt
und Clippy mit `-D warnings` sind ebenfalls erfolgreich.
Die zusätzliche Browsernachprüfung unter Linux mit Playwright 1.64.1, Node 26.13.0
und npm 12.2.1 besteht 311 Tests in Chromium, Firefox und WebKit ohne Testwiederholungen
(6,1 Minuten). Sie verwendet die produktive Darstellungskorrektur ohne CSS-Eingriffe
im Test. Die sieben bestehenden Ausnahmen sind sechs separat ausgeführte A/B-Updates
und ein vollständig netzloser WebKit-Neuladetest. Auch die zehn mobilen PWA-Tests
und die sechs separat ausgeführten Produktionsupdates bestehen ohne
Testwiederholungen. Insgesamt bestehen damit 327 Browser-/PWA-Tests unter Linux.

Die zehn kalten mobilen Lighthouse-Messungen bestehen ebenfalls. Die Mediane sind
99 Punkte / 758,6 ms FCP für die öffentliche Seite und 96 Punkte / 759,0 ms FCP für
die angemeldete Sammlung. Die Grenzen bleiben mindestens 90 Punkte und höchstens
3.000 ms FCP pro Route. Alle fünf Sammlungsläufe bestätigen 200 Einträge und geladene
Cover. [Die Linux-Messreihe](pwa-performance-results-linux.json) dokumentiert sämtliche
Einzelwerte, Werkzeuge und das Profil. Ihr Commitfeld nennt den Ausgangsstand `f693ffc`;
`workingTreeDirty: true` kennzeichnet die hier geprüften, noch nicht committeten
Korrekturen. Die ursprüngliche Messreihe vom 31.10. bleibt separat erhalten.

Der anschließende [GitHub-CI-Lauf zu `16fefed`](https://github.com/McNamara84/lilly/actions/runs/41275637846)
bestätigte alle Build-, Lint-, Backend- und Browserprüfungen. Der zusätzliche Codecov-Check
meldete noch fehlende Testabdeckung; die Nachprüfung dazu folgt unten.
Der Implementierungsplan bleibt außerhalb des Korrekturcommits.

## Codecov-Nachprüfung vom 01.11.2026

Der [Codecov-Bericht zu PR #145](https://github.com/McNamara84/lilly/pull/145#issuecomment-6553287043)
meldete 83,98268 % Patch-Abdeckung bei einer geforderten Schwelle von 93,79 %.
Der lokale Abgleich des LCOV-Berichts mit den geänderten Zeilen gegenüber `28adf4b`
reproduzierte diese Zahl exakt: 388 von 462 Zeilen waren vollständig abgedeckt.
Auch eine ausgeführte Zeile mit einer ungetesteten Verzweigung zählt dabei als Lücke.

45 zusätzliche Tests schließen die Lücken bei verspäteten Antworten nach Logout,
OAuth-Sitzungswechseln, gesperrtem Browserspeicher, unterbrochenen IndexedDB-Zugriffen,
Synchronisierungsberechtigungen zwischen Tabs, abhängigen Änderungen und der Anzeige
von Abmelde-/Updatefehlern. Die Datenbanktests verwenden `fake-indexeddb` mit tatsächlichen
Transaktionen; konkurrierende Abläufe werden an definierten Transaktionsgrenzen ausgelöst.
Produktionscode, Coverage-Konfiguration und Prüfschwellen bleiben unverändert.

Der abschließende Linux-Lauf mit Node 26.13.0 und Vitest 5.0.5 besteht alle 962 Tests
in 72 Dateien ohne Wiederholungsversuche. Für den gesamten PR sind nun **462 von 462
bewerteten geänderten Zeilen vollständig abgedeckt (100 %)**. Die gesamte Frontendmessung
erreicht 98,62 % Statements, 89,89 % Branches und 99,36 % Lines; die 100 % beziehen sich
ausdrücklich auf den Patch. ESLint, Prettier und Svelte-/TypeScript-Prüfung bestehen ebenfalls.
Ein erster Linux-Lauf überschritt beim anfänglichen Modulimport eines bestehenden Tests
das Zeitlimit, während weitere Prüfprozesse liefen. Der abschließende Lauf lief separat
mit unverändertem Zeitlimit erfolgreich durch. Die erneute Bestätigung durch Codecov
steht bis zum Push des Testcommits aus.

## Erste Review-Runde vom 01.11.2026

Alle vier Hinweise ließen sich anhand des Codes und gezielter Regressionstests
bestätigen. Die Korrekturen schützen folgende Abläufe:

- Der Online-PATCH ohne verfügbares Cache-Profil prüft nach der Antwort dieselbe
  Sitzungskennung wie die übrigen privaten Antwortpfade. Eine zwischenzeitliche
  Abmeldung oder Anmeldung verwirft die verspätete Antwort.
- Logout- und Kontolöschungsnachrichten enthalten die ursprüngliche Sitzungskennung
  und die zugehörige Abmeldekennung. Empfänger akzeptieren sie nur für diese Sitzung
  oder deren bereits gemeinsam gespeicherten Abmeldestand. Überholte Nachrichten
  verändern weder Anmeldung noch Cache. Auch eine bereits begonnene lokale Löschung
  prüft nach ihren IndexedDB-Lesezugriffen die Sitzung erneut. Ein verzögerter
  Service-Worker-Aufruf darf ebenfalls keine neuere Anmeldung bereinigen.
- Der Hefteditor registriert seinen Änderungszustand ausdrücklich. Notizen, Auflage,
  Status und Zustand bleiben bei laufendem oder fehlgeschlagenem Speichern geschützt;
  erfolgreich gespeicherte beziehungsweise verworfene Eingaben geben das Update frei.
- Das Profil vergleicht seine Eingaben mit den zuletzt gespeicherten Daten.
  Validierungsfehler und fehlgeschlagene oder laufende Speichervorgänge behalten
  den Updateschutz. Das bloße Absenden eines Formulars gilt nicht mehr als Speichern.

23 zusätzliche Unit-/Komponententests prüfen diese Abläufe einschließlich erneuter
Anmeldung desselben Kontos, gesperrtem IndexedDB, verzögerter Bereinigung, Änderungen
über Buttons und erfolgreicher Wiederholung nach einem Speicherfehler. Der vollständige
Linux-Lauf besteht **985 Tests in 72 Dateien**. Der LCOV-Abgleich gegen `28adf4b`
ergibt **506 von 506 vollständig abgedeckten geänderten Zeilen (100 % Patch-Abdeckung)**.
Die Gesamtmessung erreicht 98,65 % Statements, 90,18 % Branches und 99,39 % Lines.
Prüfschwellen und Coverage-Konfiguration bleiben unverändert. Produktionsbuild,
ESLint, Prettier und Svelte-/TypeScript-Prüfung bestehen ebenfalls.

Die Browsernachprüfung unter Linux besteht **334 Tests ohne Wiederholungen**:
314 in der vollständigen Chromium-/Firefox-/WebKit-Suite, elf mobile PWA-Tests
und neun separat ausgeführte A/B-Produktionsupdates. Die zehn Ausnahmen der
vollständigen Suite sind diese neun separat ausgeführten Updates und der bekannte
vollständig netzlose WebKit-Neuladetest. Der neue Sitzungstest meldet dasselbe Konto
über das echte Formular erneut an und stellt anschließend alte Logout- und
Kontolöschungsnachrichten zu. Der neue Updatetest prüft Änderungen im selben Tab,
fehlgeschlagene Validierung, einen fehlgeschlagenen Profilspeichervorgang und das
erfolgreiche Update nach erneutem Speichern. Weil der Hefteditor den Updatehinweis
überlagert, löst dieser Test dessen echten Klickhandler programmatisch aus; die
Profilprüfungen bedienen den sichtbaren Updatebutton direkt. Echte Gerätechecks
bleiben wie vereinbart offen. Lighthouse wurde in dieser Review-Runde nicht erneut
gemessen; die oben dokumentierte Messreihe gehört zur vorangegangenen CI-Korrektur.

Unversionierte Nachrichten aus älteren App-Versionen werden bewusst ignoriert:
Ohne die ursprüngliche Sitzungskennung lassen sie sich keiner noch gültigen Sitzung
sicher zuordnen. Die gemeinsame Abmeldesperre schützt weiterhin spätere private
Datenzugriffe; die sofortige tabübergreifende UI-Abmeldung setzt das neue
Nachrichtenformat auf beiden Seiten voraus.

## Zweite Review-Runde vom 01.11.2026

Der Lighthouse-Hinweis ist für die festgelegte Version **13.6.0 nicht zutreffend**.
Der installierte Einstiegspunkt `lighthouse/core/index.js` nimmt `page` als vierten
Parameter entgegen und übergibt ihn an `navigation(page, url, ...)`. In
`core/gather/navigation-runner.js` wird nur bei fehlendem `page` eine zusätzliche
Browserverbindung samt Seite angelegt. Der vorhandene Messrunner verwendet damit
bereits dieselbe Seite für Sitzungscookies, Navigation, Fehlerüberwachung und
Inhaltsprüfung. Die Runner-Tests prüfen jetzt ausdrücklich die Identität dieses
vierten Arguments und dass ein Fehler dieser Seite die Messung scheitern lässt.
Die Browseranbindung wurde nicht umgebaut.

Die drei Hinweise zum Sitzungsschutz sind berechtigt und durch Regressionstests
vor der Korrektur bestätigt:

- Anlegen und Bearbeiten erfassen ihre Sitzung vor dem ersten Cache-Zugriff.
  Nach verzögerten Profil- und Eintragszugriffen wird sie vor dem Online-Schreiben
  erneut geprüft. Die Antwortprüfung bleibt erhalten und gilt nun auch für den
  POST-Fallback ohne Cache-Profil.
- Eine bereits lokal abgemeldete Sitzung darf auch bei noch gültigen Servercookies
  keinen neuen POST/PATCH beginnen. Ein unveränderter, aber gesperrter Sitzungsstand
  wird deshalb ausdrücklich abgelehnt. Auch die Übergänge zwischen lokalem
  Vormerken, Synchronisieren und Rückgabe des Ergebnisses prüfen den Ursprung.
- Die Bereinigung nach einer serverseitig gemeldeten Kontolöschung erhält die
  zugehörige Abmeldekennung. Eine neuere Anmeldung während der IndexedDB-Lesezugriffe
  verhindert das Löschen. Der alte Authentifizierungsablauf endet nach der
  Bereinigung, statt anschließend Tokens zu erneuern oder die neue Anmeldung
  zurückzusetzen. Auch die Ermittlung des zu löschenden Cache-Profils und die beim
  App-Start wiederaufgenommene Bereinigung sind gegen Sitzungswechsel abgesichert.

Bei der Browsernachprüfung scheiterte ein bestehender WebKit-Test am Offline-Übergang;
auch einer von drei gezielten Vergleichsläufen scheiterte dort. Die Untersuchung
bestätigte zusätzlich einen Wettlauf im Verbindungsstatus: Eine verspätete Antwort
der Erreichbarkeitsprüfung konnte ein neueres Offline-Ereignis oder Prüfergebnis
überschreiben. Drei deterministische Unit-Tests reproduzieren diesen Fehler.
Eine laufende Erreichbarkeitsprüfung darf ihren Zustand jetzt nur übernehmen,
solange kein neueres Ereignis beziehungsweise keine neuere Prüfung sie ersetzt hat.

30 zusätzliche Regressionstests decken die Sitzungslücken und die genannten
Zeitabläufe ab. Der abschließende Linux-Lauf besteht **1.015 Tests in 72 Dateien**.
Der LCOV-Abgleich gegen `28adf4b` ergibt **535 von 535 vollständig abgedeckten
geänderten Zeilen (100 % Patch-Abdeckung)**. Insgesamt werden 98,71 % Statements,
90,36 % Branches und 99,41 % Lines erreicht. Produktionsbuild, ESLint, Prettier und
Svelte-/TypeScript-Prüfung bestehen; Prüfschwellen bleiben unverändert.

Die abschließende PWA-Browserprüfung besteht **43 Tests ohne automatische
Wiederholungen** in Chromium, Firefox, WebKit und mobilem Chrome. Der zuvor
fehlgeschlagene Offline-Übergang besteht nun ebenfalls. Ein vollständig netzloser
WebKit-Neuladetest bleibt wie bisher ausgenommen. Zusätzlich bestanden die
117 ausgewählten Browserprüfungen für Anmeldung, Sammlung und Profil in allen
drei Desktop-Engines. Die anfänglichen WebKit-Fehler sind damit ausdrücklich
von den erfolgreichen Prüfungen nach der Statuskorrektur getrennt dokumentiert.

Die erneute echte Lighthouse-Messreihe besteht alle zehn kalten Navigationen.
Alle fünf Sammlungsläufe bestätigen die angemeldete Identität, 200 Einträge und
ein geladenes Cover auf der von Lighthouse verwendeten Seite. Die unveränderte
Browseranbindung funktioniert damit auch ohne Testdouble. Die Mediane betragen
99 Performancepunkte für den öffentlichen Einstieg und 96 Punkte für die Sammlung;
beide FCP-Mediane liegen unter 800 ms. Die Grenzwerte bleiben 90 Punkte und 3.000 ms.
[Die Messreihe dieser Review-Runde](pwa-performance-results-review2.json) enthält
alle Einzelwerte und die Werkzeugversionen. `commit: 911837d…` bezeichnet den
Ausgangsstand, `workingTreeDirty: true` die geprüften Korrekturen vor dem Commit.

## Dritte Review-Runde vom 01.11.2026

Die drei Hinweise sind berechtigt und behoben:

- Der Deadlock-Test registriert Trigger und Datenbank-Fixtures sofort zur
  Bereinigung. Ein Guard entfernt sie auch beim Unwinding nach fehlgeschlagenen
  Assertions und beim Abbruch eines Tokio-Tasks, bevor der Datenbank-Testmutex
  freigegeben wird. Die Notfallbereinigung wartet auf eine unabhängige Verbindung;
  ausstehende Transaktions-Rollbacks können dabei weiterlaufen. Drei MariaDB-Tests
  prüfen Erfolg, Panic und Task-Abbruch einschließlich einer noch offenen
  Transaktion. Die Fehlerauslösung des ursprünglichen Triggers war bereits durch
  verbindungslokale Variablen isoliert; sein Verbleib im gemeinsamen Schema und
  die fehlende Fixture-Bereinigung waren trotzdem Fehler. Bei hartem Prozessabbruch
  oder unerreichbarer Datenbank kann ein Guard keine Bereinigung garantieren.
- PATCH wiederholt die vollständige Transaktion mit derselben Fehlerklassifizierung
  und maximal fünf Versuchen wie die übrigen abgesicherten Schreibvorgänge.
  Fotolöschungen bleiben außerhalb der Wiederholung und erfolgen erst nach dem
  Commit. Ein MariaDB-Regressionstest injiziert Fehler in die Tauschabstimmung:
  Er prüft ausgeschöpfte Wiederholungen, einen dauerhaften SQL-Fehler, einen
  Revisionskonflikt und schließlich den Erfolg nach zwei Deadlocks. Fehlgeschlagene
  Versuche erhalten Revision, Notizen, Fotodatensatz und Fotodatei. Der Erfolg
  erhöht die Revision genau einmal und verarbeitet genau einen Löschauftrag.
- Erfolgreiche Anmeldung setzt den alten Logout-Fehler zurück, auch über OAuth,
  Token-Erneuerung und direktes Setzen des angemeldeten Nutzers. Fehlgeschlagene
  Anmeldung erhält den Hinweis. Verspätete Fehler eines überholten Logout- oder
  Bereinigungsaufrufs dürfen ihn in einer neueren Sitzung nicht wieder einblenden.

Die Browserprüfung deckte zusätzlich auf, dass der Logout-API-Client HTTP-Fehler
bisher als Erfolg behandelte. Er prüft jetzt auch den Antwortstatus. Zusätzliche
Tests prüfen 401, 503, nicht als JSON lesbare Proxyfehler, Netzwerkfehler und eine
erfolgreiche leere 204-Antwort. Der Browser-Regressionstest simuliert eine
503-Antwort und bedient anschließend das echte Anmeldeformular zunächst mit einem
falschen, dann mit dem richtigen Passwort.

Die vollständigen Prüfungen bestehen **436 Rust-Tests mit angebundener MariaDB**
und **1.026 Frontend-Tests in 72 Dateien**. Die vier neuen Rust-Tests und elf neuen
Frontend-Tests sichern die oben beschriebenen Abläufe ab. Der LCOV-Abgleich gegen
`28adf4b` ergibt **542 von 542 vollständig abgedeckten geänderten Frontend-Zeilen
(100 % Patch-Abdeckung)**. Insgesamt werden 98,71 % Statements, 90,38 % Branches
und 99,41 % Lines erreicht. Rustfmt, Clippy mit `-D warnings`, Prettier, ESLint,
Svelte-/TypeScript-Prüfung und Frontend-Produktionsbuild bestehen. Prüfschwellen
und Coverage-Konfiguration bleiben unverändert.

Die abschließende Browserprüfung besteht **36 PWA-Resilienztests ohne automatische
Wiederholungen** in Chromium, Firefox, WebKit und mobilem Chrome. Im ersten Lauf
scheiterte der neue Logout-Test in allen vier Projekten am unbeachteten HTTP-Fehler;
nach der Statusprüfung bestehen diese vier Fälle ebenso wie die 32 bestehenden
Prüfungen. Lighthouse und die separate A/B-Updatereihe wurden in dieser Runde
nicht erneut ausgeführt; ihre zuletzt dokumentierten Messungen bleiben unverändert.

## Browsergrenzen und spätere Geräteprüfung

Playwright-WebKit kann beim vollständig netzlosen Neuladen einer vom Service Worker
kontrollierten Seite einen internen Browserfehler liefern. Die betroffenen Reloads und
direkten Offline-Navigationen sind dort ausgenommen. Chromium und Firefox prüfen sie;
WebKit prüft weiterhin Offline-Schreiben, Kataloglesen innerhalb der App, Reconnect,
Konflikte, tabübergreifenden Logout und echte Produktionsupdates. Die Ausnahmen sind
keine Aussage über die Funktionsfähigkeit auf realen Safari-Geräten.

- [ ] Android: Installation aus dem Browser, Start über Homescreen, Offline-Kaltstart.
- [ ] iPhone/iPad: „Zum Home-Bildschirm“, eigenständiger Start, Offline-Kaltstart.
- [ ] Beide Plattformen: App beenden/neu öffnen, ausstehende Änderungen erhalten,
  Reconnect und Update mit ungespeicherten Eingaben.
- [ ] Speicherknappheit, Betriebssystembereinigung und lange Offline-Zeiträume prüfen.

## Abgleich mit #19

[Issue #19](https://github.com/McNamara84/lilly/issues/19) ist seit 31.10.2026 als
abgeschlossen markiert. Seine beiden Kommentare enthalten keinen vollständigen
John-Sinclair-Erst-/Wiederholungsnachweis. Der vorhandene lokale Bericht zu Job 7
enthält weiterhin `completed_with_errors`, zwei fehlgeschlagene Hefte und blockierte
Freigabe. Das bestätigt keinen späteren erfolgreichen Vollimport.

Die offenen Nachweise in [der MVP-Importabnahme](mvp-acceptance.md) bleiben deshalb
unverändert. Die deterministischen PWA-Fixtures ersetzen weder Quellprüfung noch
Live-Importabnahme. In diesem Paket wurde kein Live-Import neu gestartet und kein
Issue auf GitHub geändert.
