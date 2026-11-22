# Abnahme der Sammlungssuche für Issue #57

Dieser Bericht dokumentiert die Umsetzung von
[NFR-PERF-002](https://github.com/McNamara84/lilly/issues/57) und das vereinbarte
Abnahmeverfahren für `GET /api/v1/me/collection`.

## Messvertrag

Der folgende vollständige Messvertrag gilt für das Profil `full`. Pull Requests
verwenden inzwischen die unten beschriebene Kurzprüfung `smoke`; sie ersetzt
keinen vollständigen Abnahmenachweis.

Jedes Szenario muss **p95 < 500 ms** erreichen, sowohl mit einer als auch mit fünf
gleichzeitig laufenden Anfragen. Gemessen wird die vollständige lokale HTTP-Antwort
einschließlich regulärer Cookie-/JWT-Authentifizierung, Kontostatusprüfung, SQL und
JSON-Ausgabe. Nach 20 geprüften Aufwärmanfragen folgen jeweils 200 Messwerte. p95
ist der 190. Wert der sortierten Stichprobe. Ein Wert von genau 500 ms besteht nicht.

Anmeldung, Containerstart, Fixture-Aufbau, SQL-Diagnostik, Browserdarstellung und
Internetlaufzeiten gehören nicht zur gemessenen Antwortzeit. Builds und Browsertests
laufen vor beziehungsweise nach der HTTP-Messung. Jeder Worker verwendet eine
eigene Verbindung und hält höchstens eine Anfrage offen. Das Verfahren prüft die
festgelegte Parallelität, keine unbegrenzte Ankunftsrate.

Jede Antwort wird gegen ein unabhängig in Python berechnetes Ergebnis geprüft:
Trefferzahl, Reihenfolge, Pagination, Heft- und Exemplardaten, Notizen und Zeitstempel.
HTTP-/JSON-Fehler, falsche Ergebnisse, fehlende Messwerte, ein überlasteter
Lastgenerator und Überschreitungen führen zum Fehlerstatus. Es gibt keine
automatischen Wiederholungen oder verworfenen langsamen Antworten.

## Referenzdaten und Szenarien

Die deterministische Fixture-Version 2 enthält 7.000 aktive Kataloghefte in zwei
Serien sowie eine zusätzliche inaktive Serie. 5.000 verschiedene gesammelte Hefte
ergeben mit einem bis drei Exemplaren genau **10.000 eigene Einträge**. Die Daten
variieren Besitzstatus, Zustandsnoten einschließlich zulässiger fehlender Noten,
Titel, Autoren, Mitautoren, Coverzeichner, Umlaute, Akzente, Erfassungsdatum,
Editionsbezeichnung und Notizlänge.

Der erste Durchlauf verwendet nur diese Sammlung. Danach werden **100.000 fremde
Einträge** von 100 Konten mit unterschiedlichen Besitzstatus und Zustandsnoten
ergänzt. Beide Phasen verwenden dieselben 63 Szenarien und dieselbe Ergebnisreferenz.
Das ergibt **252 Messreihen mit 50.400 gemessenen Antworten** und zusätzlich 5.040
Aufwärmanfragen. SQL-Diagnostik und Browserprüfungen zählen nicht zu diesen Werten.

| Gruppe | Geprüfte Varianten |
| --- | --- |
| Grundansicht | Standardseite, volle Seite mit 100 Treffern, beide Serien separat |
| Besitz und Zustand | `owned`, `duplicate`, `wanted`, Z0 bis Z4, Zustandsbereich |
| Heftauswahl | Heftnummer und konkrete Heft-ID |
| Teilstringsuche | Häufige, seltene und fehlende Titel-/Autorenbegriffe, Akzente und Umlaute, allgemeines `q` |
| Sortierung | Heftnummer, Serie, Zustand, Titel, Autor und Erfassungsdatum jeweils auf- und absteigend |
| Kombinationen | Serie/Status/Zustand, Serie/Titel, Serie/Autor, Autor/Zustand, Suche mit Autorensortierung |
| Pagination | Seiten 2, 100, 200 und 201, letzte Autorenseite, Seitengrößenbegrenzung, Normalisierung von 0, Offset-Begrenzung |
| Fehlende Hefte | Beide Serien, Titel-/Autorensuche sowie alle zulässigen Sortierungen in beiden Richtungen |

Die vollständigen Parameter stehen in `scripts/collection_fixture.py`. Fachliche
Negativfälle und ungültige Sortierungen werden von den bestehenden Backend-Tests
geprüft und nicht als schnelle Treffer in die Performance-Statistik aufgenommen.

## Technische Änderungen

Die Trefferabfrage sortiert zunächst nur Eintrags-IDs und den benötigten Sortierwert.
Erst nach `LIMIT`/`OFFSET` werden die vollständigen Daten der Ergebnisseite geladen.
Dadurch müssen lange Notizen und andere breite Felder nicht für sämtliche
Kandidaten materialisiert werden.

Die äußere Abfrage verwendet `STRAIGHT_JOIN`, damit MariaDB die bereits begrenzte
Ergebnisseite vor den vollständigen Sammlungs-, Heft- und Seriendaten liest. Bei
tiefen Offsets konnte der Optimierer diese Reihenfolge zuvor umkehren und trotz
der Seitenauswahl nochmals alle 10.000 vollständigen Sammlungseinträge lesen.
Der Performance-Runner prüft deshalb in beiden Datenphasen zusätzlich, dass die
äußeren Sammlungszugriffe höchstens der normalisierten Seitengröße entsprechen.
Die Kandidatensuche innerhalb der materialisierten Seite und der gesonderte
Anti-Join für fehlende Hefte bleiben davon ausgenommen.

Für die Autorensortierung wird der kleinste Autorenname unter der vereinbarten
Kollation einmal je eigenem logischen Heft ermittelt. Nur diese Namen und der
leere Sentinel gehen in die Rangberechnung ein. Personen ohne Bezug zur eigenen
Sammlung vergrößern das Sortierfenster daher nicht. Ein numerischer Rang ersetzt
wiederholte Vergleiche breiter Namen.
`DENSE_RANK` mit `utf8mb4_unicode_ci` erhält die Gleichbehandlung von Groß-/Klein-
schreibung und Akzenten. Ein leerer Sentinel erhält die bisherige Sortierung von
fehlenden beziehungsweise leeren Autorennamen. Der neue Index
`idx_issue_persons_role_issue_person` unterstützt den Zugriff auf Autorenbeziehungen.
Filterwerte bleiben gebundene Parameter; die bisherigen stabilen Gleichstandsregeln
und die Nutzerabgrenzung bleiben erhalten.

Auch die Seriennamen erhalten für die Kandidatensortierung einmal je Abfrage einen
numerischen Rang unter der vorhandenen Spaltenkollation. So vergleicht insbesondere
die tiefe Seitennavigation kurze Zahlen statt wiederholt breite Seriennamen. Die
vollständigen Namen werden erst für die Ergebnisseite geladen. Zulässige Namen mit
Groß-/Kleinschreibung und Akzenten sowie stabile Reihenfolgen mehrerer Exemplare
werden zusätzlich über mehrere Seiten und Sortierrichtungen geprüft.

Für jede Variante werden die tatsächlich ausgeführten Zähl- und Trefferabfragen
über das Datenbankprotokoll erfasst. `EXPLAIN FORMAT=JSON` und
`ANALYZE FORMAT=JSON` laufen anschließend außerhalb der Zeitmessung. Vor der
Erfassung wird der Backend-Verbindungspool bei bereits aktivierter Protokollierung
neu gestartet: MariaDB kann bei zuvor ohne Protokollierung vorbereiteten Abfragen
sonst die Platzhalter statt der eingesetzten Parameter ausgeben. Der dynamische
Loopback-Port wird danach erneut ermittelt. Die Protokollierung ist während aller
Zeitmessungen ausgeschaltet. Der Runner prüft in jeder konfigurierten Datenphase
die vollständige Matrix aus Szenario und Abfrageart sowie die Struktur der
ausgeführten Pläne und gültige Zeilenmetriken. Nach Ergänzen fremder Daten prüft
er zusätzlich den Nutzerindexzugriff und die tatsächlich
besuchten Sammlungszeilen einschließlich wiederholter Join-Durchläufe. Die Grenze
beträgt je Sammlungsalias 10.000 Zeilen mit 5 % Rundungsmarge plus 100 Zeilen für
Seitenzugriffe. Begrenzte Primärschlüsselzugriffe auf die bereits ausgewählte Seite
sind zulässig. Nullmal ausgeführte Teilpläne werden nicht als Scan gewertet.

Im Frontend liest eine bestätigte Online-Sitzung bei unveränderter Sammlung nur
die Warteschlangenzähler und die Einträge der aktuellen Ergebnisseite. Eine
gemeinsame IndexedDB-Schreibtransaktion über Sammlung, Änderungen und Konflikte
verbindet diese Prüfung mit dem Cache-Abgleich. Andere Tabs können Konflikte damit
nicht zwischen getrennten Prüfungen in die Warteschlange verschieben. Erkennt
die Transaktion lokale Änderungen oder behält sie einen ausstehenden Eintrag,
werden die lokalen Ergebnisse angezeigt. Ohne ausstehende Änderungen entfällt
das Lesen der gesamten Sammlung. Eine laufende Snapshot-Aktualisierung desselben
Tabs wird für den Seitenabgleich vollständig zurückgerollt und nach einer Sekunde
erneut versucht. So blockiert ihr langer Cache-Schreibvorgang insbesondere in
WebKit keine neue Suche. Der vorherige Snapshot und lokale Änderungen bleiben
erhalten; ein Sitzungswechsel verhindert die Wiederholung mit alten Daten.
Sitzungswechsel und abgebrochene Anfragen werden nach asynchronen Schritten
erneut geprüft, auch bei WebKit, das abgebrochene Anfragen als `TypeError` melden
kann.

## Lokale Referenzumgebung

Der historische lokale Abnahmelauf `4bb9a57d0d964d659b65be7d52827495` baut die geänderten Quellen auf
Basis von `0c539bf0fce8b3961b47b4c10b73bb72d62d78a4`. Das Protokoll kennzeichnet
den Arbeitsstand ausdrücklich als geändert; der abschließende Commit entsteht nach
den Prüfungen. Änderungen an Bericht und Tests nach dem Build verändern das
gemessene Release-Binary nicht.

| Bestandteil | Tatsächlich geprüftes Profil |
| --- | --- |
| Host | Windows 11, AMD Ryzen 7 3800X, Docker Desktop mit WSL2-Kernel 6.6.87.2 |
| Docker-VM | 16 logische CPUs, 16.727.080.960 Byte RAM |
| Backend | Rust 1.100.0, Release-Build, 1 CPU, 512 MiB RAM |
| Datenbank | MariaDB 13.0.2, 1 CPU, 1 GiB RAM, 256 MiB InnoDB-Buffer-Pool |
| Lastgenerator | Python 3.14.0 auf dem Host |
| Docker Client/Server | 29.9.3 / 29.9.3 |
| Fixture | Version 2, Seed 0, Manifest-SHA-256 `957f9eb724a96d637f9d00fdd15a84b802785066e8947a09d8ee49c708354fae` |

Containerlimits werden über Docker-Inspect und den tatsächlichen Datenbankwert
geprüft. Gleiche Limits garantieren keine identische CPU-Leistung auf anderen
Hosts. Der dokumentierte lokale Lauf ersetzt deshalb den späteren CI-Lauf nicht.

Der reguläre Ratenbegrenzer läuft mit dem ausschließlich im Messprofil erhöhten
Budget von 1.000.000 authentifizierten API-Anfragen pro Minute. Anmeldung und
Sitzungskontrolle bleiben regulär; Demo-Seeding, E2E-Adapter und Import-Scheduler
sind deaktiviert.

Der ursprüngliche Code verfehlte die Grenze bereits ohne fremde Einträge bei
aufsteigender Autorensortierung mit fünf parallelen Anfragen: **p95 790,048 ms**.
Dieser Ausgangslauf wurde am ersten Fehler beendet; er ist kein vollständiger
Baseline-Nachweis. Die eigene Sammlung und das Ressourcenprofil entsprachen der
Abnahme; die spätere Fixture-Version 2 variiert zusätzlich die Statusdaten fremder
Sammlungen. Der ursprünglich pro Exemplar ausgeführte Autoren-Unterselect wurde
dabei rund 10.000-mal ausgewertet.

Eine Zwischenversion verfehlte die Grenze bei der letzten Autorenseite mit fünf
parallelen Anfragen (**p95 602,476 ms**). Das führte zur zusätzlichen Optimierung
der Seriennamen in den Sortierschlüsseln. Dieser fehlgeschlagene Lauf bleibt als
Diagnose erhalten und wird nicht mit dem abschließenden Abnahmelauf vermischt.

## Messergebnis der ursprünglichen lokalen Vollabnahme

Der lokale Lauf `4bb9a57d0d964d659b65be7d52827495` ist **bestanden**. Alle 252 Messreihen
enthalten genau 200 gültige Antworten ohne Fehler. Jede Reihe erfüllt separat
**p95 < 500 ms**. Die folgende Tabelle zeigt jeweils den langsamsten Szenario-p95
der betreffenden Gruppe; es handelt sich nicht um ein Gesamtperzentil.

| Datenbestand | Parallelität | Messreihen | Langsamstes Szenario | Höchster p95 |
| --- | ---: | ---: | --- | ---: |
| 10.000 eigene Einträge | 1 | 63 | Breite Suche mit Autorensortierung | 70,914 ms |
| 10.000 eigene Einträge | 5 | 63 | Breite Suche mit Autorensortierung | 387,921 ms |
| Zusätzlich 100.000 fremde Einträge | 1 | 63 | Letzte Seite mit Autorensortierung | 71,030 ms |
| Zusätzlich 100.000 fremde Einträge | 5 | 63 | Besitzstatus `owned` | 410,528 ms |

Die reguläre aufsteigende Autorensortierung erreicht mit fünf parallelen Anfragen
304,112 ms ohne und 377,160 ms mit fremden Einträgen. Die letzte Autorenseite erreicht
303,072 beziehungsweise 383,882 ms. Ihre Trefferabfrage benötigt im ausgeführten
SQL-Plan rund 19,9 beziehungsweise 26,2 ms. Die reguläre Autorentrefferabfrage liegt
bei rund 19,8 beziehungsweise 19,4 ms gegenüber rund 62,4 ms im Ausgangscode.

Nach Ergänzen fremder Daten besuchen die ausgeführten Pläne höchstens rund 10.100
Zeilen je `ce`-Alias einschließlich Seitenzugriffen; die Autorenquelle
`ce_authors` besucht höchstens 10.000 eigene Zeilen. Alle 126 Zähl-/Trefferpläne
dieser Phase bestehen die Prüfung der Nutzerindizes und Zeilengrenzen. Der höchste
CPU-Anteil des Lastgenerators liegt bei rund 0,211 eines Kerns, unter seiner
Überlastungsgrenze von 0,9 für fünf Worker.

Die anschließende Prüfung verzögerter Suchantworten besteht in **Chromium, Firefox
und WebKit mit 10.000 eigenen Einträgen**. Container und Volumes des Laufs wurden
erfolgreich entfernt. Rohwerte, sämtliche SQL-Pläne und die vollständige Tabelle
stehen lokal in `performance-results/collection/results.json` beziehungsweise
`performance-results/collection/summary.md`.

## Prüfungen und Testabdeckung der ursprünglichen Vollabnahme

| Prüfung | Ergebnis |
| --- | --- |
| Rust-Workspace mit echter MariaDB | 439 Tests erfolgreich; lokal unter Windows und in der Linux-Coverage-Umgebung |
| Rust-Coverage unter Linux | 87,99 % Zeilen im Workspace; alle 37 ausführbaren Regionen des geänderten Abfragekörpers und alle 9 Regionen der Sortierfeldfunktion abgedeckt |
| Frontend mit Vitest | 1.040 Tests erfolgreich; 98,79 % Statements, 90,60 % Branches, 99,48 % Zeilen |
| Python-Suite | 79 Tests, davon 25 plattformbedingt übersprungen; 40 Tests für Fixture, Ergebnisreferenz und Messwerkzeug |
| Python-Coverage der neuen Module | Jeweils 100 % Statements und Branches für Fixture und Performance-Runner |
| Sammlung und PWA in Chromium, Firefox und WebKit | 122 Tests erfolgreich, 1 vorhandener plattformabhängiger Skip, keine Wiederholungen |
| Verzögerte Suche mit 10.000 eigenen Einträgen | Je ein erfolgreicher zusätzlicher Test in Chromium, Firefox und WebKit, keine Wiederholungen |
| Statische Prüfungen | Rustfmt, Clippy mit Warnungen als Fehler, OpenAPI-Abgleich, Prettier, ESLint und Svelte-/TypeScript-Prüfung erfolgreich |

Die neue Datenbankprüfung deckt unter anderem mehrere Exemplare und Autoren,
fehlende und leere Autorennamen, kollationsgleiche Namen, stabile Gleichstände,
lange private Notizen, fremde Konten, inaktive Serien, Filterkombinationen und
späte Seiten ab. Die Frontend-Tests halten alte Antworten gezielt zurück und prüfen
Erfolg, Fehler und Abbruch überholter Anfragen sowie Ladezustand und Unmount.
Zusätzlich werden Debounce, verzögerte Cache-Aktualisierung, lokale Änderungen,
Konflikte, Logout und Sitzungswechsel geprüft. Die vorhandenen Tests für
Offlinebetrieb und mehrere Browser-Tabs bleiben erfolgreich.

Ein vorhandener Importtest verwendet jetzt das plattformgerechte temporäre
Verzeichnis, damit er auch unter Windows seinen Cover-Download prüfen kann.

## Korrekturen aus der ersten Review-Runde

Die drei Review-Befunde sind durch Regressionstests abgesichert:

- Zwei getrennte IndexedDB-Verbindungen verschieben einen Konflikt zurück in die
  Warteschlange beziehungsweise speichern eine Änderung unmittelbar vor dem
  Seitenabgleich. In beiden Fällen erscheinen die lokalen Änderungen. Weitere
  Fälle prüfen leere Ergebnisseiten, andere Konten, Transaktionsabbruch und
  Sitzungswechsel vor und beim Commit. Die Snapshot-Unterbrechung wird mit
  Änderungen vor und während der Pause sowie Logout und erneuter Anmeldung
  geprüft; echte Speicherfehler bleiben Fehler.
- Eine echte MariaDB-Abfrage mit 10.000 zusätzlichen, unbeteiligten Personen
  verarbeitet für ein Konto mit einem gesammelten Heft im Autorenfenster genau
  zwei Zeilen: das eigene Heft und den Sentinel. Die bestehenden Prüfungen für
  leere Namen und kollationsgleiche Gleichstände bleiben erhalten. Nur Hefte mit
  Autorenbeziehungen benötigen einen Namen in der Rangquelle; fehlende Autoren
  erhalten über den Seiten-Join denselben Rang wie der leere Sentinel.
  Autorenabfragen beginnen bei der eigenen Sammlung.
- Leere, unvollständige, doppelte oder strukturell ungültige Planlisten werden
  auch für `own-only` abgelehnt. Ungültige Ausführungsmetriken scheitern in beiden
  Phasen; Nutzerindex und Scan-Grenze werden weiterhin nur für `with-foreign`
  erzwungen. Der historische vollständige Messbericht besteht auch die neue
  Strukturprüfung.

Die 439 Rust-Tests mit echter MariaDB und alle 1.056 Frontend-Tests bestehen.
Die Frontend-Abdeckung beträgt 98,82 % Statements und 90,63 % Branches. Alle neu
hinzugekommenen ausführbaren Zeilen im IndexedDB-Modul sind abgedeckt. Die
Python-Suite umfasst jetzt 88 Tests mit 25 plattformbedingten Skips; die 49 Tests
für Fixture und Performance-Runner decken beide Module weiterhin zu 100 % bei
Statements und Branches ab. Rustfmt, Clippy, Prettier, ESLint und die
Svelte-/TypeScript-Prüfung sind erfolgreich.

Auch die erneuten Sammlung- und PWA-Prüfungen in Chromium, Firefox und WebKit
bestehen: 122 erfolgreiche Tests, ein vorhandener plattformabhängiger Skip und
keine Wiederholungen. Die verzögerte Suche mit 10.000 eigenen Einträgen besteht
zusätzlich in allen drei Browsern.

Der gemeinsame Abschlusslauf `fba0a4f0deec43cfa590f3265e43a96f` mit `--profile
smoke --browser-check` ist bestanden: zwölf Messreihen mit je 200 gültigen
Antworten sowie die große Sammlung in allen drei Browsern. Höchster p95:
**492,923 ms** bei der kombinierten Suche mit Autorensortierung. Aufsteigende
Autorensortierung: **394,648 ms**, absteigende: **392,443 ms**, letzte Autorenseite:
**405,895 ms**. Einschließlich Browser-Build, Browserprüfung und Aufräumen dauerte
der Lauf **232,170 Sekunden (3 Minuten 52 Sekunden)**; der Backend-Build war im
Docker-Cache. Die Rohdaten liegen in
`performance-results/collection-review-accepted/`. Dieser Kurzlauf ersetzt
keine vollständige Abnahme aller 252 Messreihen.

Beim Nachtesten wurde außerdem das Aufräumen des optionalen Browserprofils
korrigiert: Der Runner entfernt nun auch dessen Container und Volumes.

## Korrektur aus der zweiten Review-Runde

Der lokale Comparator verwendete für `sort=author` bisher den Serien-Fallback.
Außerdem kehrte `desc` auch die Gleichstandsregeln um. Das betraf Online-Antworten,
sobald eine ausstehende lokale Änderung oder ein Konflikt die Anzeige aus dem
Cache erforderte.

Die lokale Autorensortierung bestimmt jetzt einmal je gefiltertem logischen Heft
den kleinsten Autorennamen. Groß-/Kleinschreibung, Akzente und nachgestellte
ASCII-Leerzeichen werden bei den Textvergleichen gleichbehandelt; fehlende und
leere Autoren verwenden den leeren Sortierschlüssel. Nur der gewählte
Primärschlüssel wechselt die Richtung. Serie, Heftnummer und Eintrags-ID bleiben
als sekundäre Schlüssel aufsteigend. Virtuelle fehlende Einträge verwenden
abschließend die Heft-ID. Standardreihenfolge, fehlende Zustandsnoten gemäß
SQL-`FIELD` und zeitlich gleiche Datumswerte sind ebenfalls angeglichen.

20 neue Unit- und Integrationstests prüfen beide Richtungen aller Sortierungen,
unsortierte Mitautorenlisten, kollationsgleiche Namen, leere Autoren, mehrere
Exemplare und Gleichstände über Seitengrenzen. Der echte API-Client wird mit
ausstehenden Änderungen und Konflikten über fünf Seiten geprüft. Die vollständige
Frontend-Suite besteht mit 1.076 Tests; das lokale Sortiermodul erreicht 100 %
Zeilen- und Funktionsabdeckung sowie 98,02 % Branches. Die gesamte
Frontend-Abdeckung beträgt 98,82 % Statements und 90,67 % Branches.

Die Browser-Sortierprüfung initialisiert den Cache aus dem echten API-Snapshot
und ändert ausschließlich eine lokale Notiz. Dadurch bleiben die Sortierschlüssel
unverändert und die sichtbaren Eintrags-IDs lassen sich direkt mit der API
vergleichen. Beide Richtungen und die ersten zwei Seiten werden jeweils mit
ausstehender Änderung und Konflikt geprüft. Zusätzlich laufen die vorhandenen
Sammlungs- und PWA-Prüfungen mit regulärem Cache-Aufbau.

Die zusätzliche lokale Prüfung mit 10.000 eigenen und 100.000 fremden Einträgen
besteht in Chromium, Firefox und WebKit: sechs Browserprüfungen für die
Autorenreihenfolge mit beiden lokalen Zuständen und für verzögerte Suchantworten,
ohne Wiederholungen. Der große Testcache wird aus dem echten Snapshot vorbereitet;
seine Initialisierung gehört nicht zur gemessenen API-Latenz. Die Rohdaten stehen
in `performance-results/ordering-large-accepted-browser.json` und
`performance-results/ordering-large-accepted-playwright.json`.

Ein vorhandener Exemplartest wartet nun auf die benötigten Katalogreferenzen im
Cache, bevor er ein weiteres Exemplar anlegt. Seine Erfolgsbedingungen für
Speichern, Anzeigen, Ändern und Löschen bleiben unverändert.
Die abschließende Sammlung-/PWA-Suite in Chromium, Firefox und WebKit besteht mit
125 erfolgreichen Tests, einem vorhandenen Plattform-Skip und ohne Wiederholungen.

## Reproduktion und CI

Für die reine API-Prüfung reichen Docker Compose und Python ab 3.12:

```bash
python scripts/collection_performance.py
```

Die kurze PR-Prüfung verwendet denselben vollständigen Datenbestand mit 10.000
eigenen und 100.000 fremden Einträgen, ausschließlich fünf Worker und weiterhin
20 Aufwärmanfragen sowie 200 Messwerte je Szenario. Sie misst genau 2.400 Antworten
in zwölf fest definierten Szenarien:

```bash
python scripts/collection_performance.py --profile smoke
```

Enthalten sind Grundansicht, volle Seite, Besitzstatus `owned`, Zustandsbereich,
breite allgemeine Suche, Titel- und Autorensortierung in beiden Richtungen, breite
Suche mit Autorensortierung, letzte Autorenseite und Autorensortierung fehlender
Hefte. Die Namen und Parameter werden aus derselben vollständigen Fixture gewählt.
Ergebnisreferenz, Nutzerindexprüfung, Ressourcenlimits und **p95 < 500 ms** bleiben
verbindlich. Die Berichte nennen das Profil ausdrücklich; eine Kurzprüfung kann
nicht als vollständige Matrix oder Skalierungsvergleich bestehen.

Der erste vollständige CI-Lauf
[44980059617](https://github.com/McNamara84/lilly/actions/runs/44980059617/job/137078567787)
verfehlte bei aufsteigender Autorensortierung mit fünf Workern und fremden Einträgen
die Grenze: **p95 506,830 ms**. Er belegt keine erfolgreiche CI-Abnahme. Für mehr
Abstand zur Grenze fasst die ungefilterte Zählabfrage eigene Exemplare jetzt zuerst
über einen abgedeckten Nutzerindex je Heft zusammen und prüft anschließend die
aktiven Serien. Zusätzliche Filter verwenden weiterhin die bisherige Abfrage.
Die Korrektur wird auch in der Kurzprüfung durch die betroffenen Autorenfälle
kontrolliert.

Die neue Profilaufteilung wurde zusätzlich lokal geprüft: 439 Rust-Tests sowie
85 Python-Tests mit 25 plattformbedingten Skips bestehen. Die 46 Tests für Fixture
und Messwerkzeug erreichen 100 % Statements und Branches in beiden Python-Modulen.
Sie prüfen die vollständige und die kurze Matrix, die unveränderten Stichproben
und Grenzwerte, fehlende/zusätzliche Messreihen, den Verbleib des langsamen
Autorenfalls sowie einen tatsächlichen Fehlerstatus bei genau 500 ms.

Der neue lokale Kurzlauf `211a613305a04b33900e732132211b7a` besteht mit genau zwölf
Messreihen und 2.400 Antworten. Einschließlich Aufbau und Aufräumen benötigt er
**167,413 Sekunden (2 Minuten 47 Sekunden)**. Höchster p95: **397,485 ms** beim
Besitzstatus `owned`; aufsteigende Autorensortierung: **295,849 ms**, letzte
Autorenseite: **302,515 ms**. Der Release-Build war bereits im lokalen Docker-Cache.
Die Rohwerte liegen in `performance-results/collection-smoke/`. Diese Kurzprüfung
ist ein eigener Nachweis für die ausgewählten Fälle und keine Wiederholung der
vollständigen Abnahme. Die Laufzeit im GitHub-Runner wird separat geprüft.

Für die ergänzende Prüfung der großen Sammlung in drei Browsern:

```bash
cd frontend
npm ci
npx playwright install --with-deps chromium firefox webkit
cd ..
python scripts/collection_performance.py --browser-check
```

Der Runner erzeugt ein eigenes Compose-Projekt mit zufälligen Zugangsdaten,
Loopback-Port und privaten Volumes. Er akzeptiert keine beliebige Ziel-Datenbank und
prüft vor dem Einspielen, dass seine Datenbank leer ist. Anschließend entfernt er
seine Container und Volumes, auch nach einem Fehler. JSON und Markdown unter
`performance-results/collection/` enthalten Lauf-ID, Basis-Commit, Änderungsstatus,
Fixture-Prüfsumme, Versionen, tatsächliche Ressourcenlimits, SQL-Pläne und sämtliche
Latenzwerte. Unvollständige Läufe behalten den Fehlerstatus.

Der wiederverwendbare Workflow `.github/workflows/collection-performance.yml` führt
bei Pull Requests `smoke` und auf `main` verpflichtend `full` aus. Die vollständige
Variante umfasst auch die große Sammlung in drei Browsern. Die Kurzprüfung benötigt
in diesem Job weder Node noch Browser-Downloads; die regulären E2E-Jobs bleiben
erhalten. Beide Varianten verwenden ein aus dem geprüften Commit gebautes
Release-Image und bewahren Ergebnisartefakte für 14 Tage auf. Manuelle Starts
verwenden standardmäßig `full` und bieten beide Profile zur Auswahl an. Auf `main`
hängt die Veröffentlichung der Release-Images vom erfolgreichen vollständigen
Performance-Job ab; dadurch hängt auch das anschließende Deployment davon ab.
`--skip-build` ist für einen sauberen CI-Checkout mit passendem OCI-Revision-Label
reserviert.

Der stabile Prüfjob heißt `Collection Performance / Collection search p95`. Eine
zusätzliche Merge-Sperre über Required Checks muss in den GitHub-Branchregeln
konfiguriert sein. Die obigen ursprünglichen Vollmesswerte dokumentieren den lokalen
Referenzlauf; der fehlgeschlagene erste CI-Lauf und spätere Kurzprüfungen sind
getrennt davon zu bewerten.

## Korrektur der CI-Regression vom 22. November 2026

Der Lauf [44996200248](https://github.com/McNamara84/lilly/actions/runs/44996200248)
bestand die letzte Autorenseite mit fünf parallelen Anfragen bei **400,300 ms**.
Der folgende Lauf
[45001688401](https://github.com/McNamara84/lilly/actions/runs/45001688401/job/137144407848)
scheiterte dort bei **692,184 ms**, obwohl der einzige neue Commit lediglich
`.gitignore` änderte. Die Berichte nennen unterschiedliche Runner-Prozessoren:
Intel Xeon 6975P-C beziehungsweise AMD EPYC 7763. Die Containerlimits und das
Datenbank-Image waren identisch. Gleiche CPU-Quoten stellen somit keine gleiche
Rechenleistung sicher.

Beide CI-Pläne zeigen außerdem die ungünstige äußere Join-Reihenfolge der letzten
Autorenseite: Zuerst wurden 7.001 Kataloghefte und rund 10.000 vollständige
Sammlungseinträge besucht, erst anschließend die 50 ausgewählten Eintrags-IDs.
Die schnellere Umgebung verdeckte diesen zusätzlichen Aufwand im ersten Lauf.
Die oben beschriebene äußere `STRAIGHT_JOIN`-Korrektur hält die Seitenauswahl vorn.
Die zusätzliche Planprüfung lehnt beide alten CI-Pläne unabhängig von der
gemessenen Laufzeit ab. Regressionstests decken beide Datenphasen, normalisierte
Seitengrößen und den getrennten Abfragepfad für fehlende Hefte ab. Die Stichproben,
Parallelitäten und die strikte Grenze von 500 ms bleiben unverändert.

Der lokale HTTP-Volltest `39586e060ee14fc3a7ec9d340aad4582` auf einem AMD Ryzen 7
3800X besteht mit **252 Messreihen und 50.400 geprüften Antworten**. Er verwendet
eine isolierte Kopie von `518dbfd2babcedfbbd96b80ebad2d93be8386071` mit der
SQL-Korrektur; das Protokoll kennzeichnet den Quellstand als geändert. Andere
unfertige Arbeitskopie-Änderungen sind nicht enthalten. Die bestehenden CPU-,
Speicher- und Datenlimits gelten unverändert.

| Datenbestand | Parallelität | Messreihen | Höchster p95 |
| --- | ---: | ---: | ---: |
| 10.000 eigene Einträge | 1 | 63 | 101,191 ms |
| 10.000 eigene Einträge | 5 | 63 | 493,887 ms |
| Zusätzlich 100.000 fremde Einträge | 1 | 63 | 78,510 ms |
| Zusätzlich 100.000 fremde Einträge | 5 | 63 | 490,902 ms |

Die letzte Autorenseite erreicht mit fünf Workern **484,391 ms** ohne und
**406,467 ms** mit fremden Einträgen. Alle ausgeführten SQL-Pläne erfüllen auch
die zusätzliche Begrenzung der äußeren Seitenzugriffe. Rohwerte und Pläne liegen
unter `.tmp/ci-performance-20261122/fixed-full/`. Dies ist ein lokaler HTTP-Nachweis;
der nächste CI-Lauf einschließlich Browserprüfung erfolgt nach Commit und Push.
