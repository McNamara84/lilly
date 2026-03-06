# LILLY – Listing Inventory for Lovely Little Yellowbacks

## Anforderungskatalog

**Version 1.0** | Stand: 06. März 2026 | Autor: Holger Ehrmann

---

## Inhaltsverzeichnis

1. [Projektübersicht](#1-projektübersicht)
2. [Rahmenbedingungen](#2-rahmenbedingungen)
3. [Funktionale Anforderungen](#3-funktionale-anforderungen)
4. [Nicht-funktionale Anforderungen](#4-nicht-funktionale-anforderungen)
5. [Zustandsbewertungsskala für Heftromane](#5-zustandsbewertungsskala-für-heftromane)
6. [Datenmodell (Übersicht)](#6-datenmodell-übersicht)
7. [Abgrenzungen](#7-abgrenzungen)
8. [Roadmap (Entwurf)](#8-roadmap-entwurf)
9. [Glossar](#9-glossar)

---

## 1. Projektübersicht

LILLY (Listing Inventory for Lovely Little Yellowbacks) ist eine Open-Source-Webanwendung zur Verwaltung und zum Tausch von Heftroman-Sammlungen im deutschsprachigen Raum. Die Anwendung richtet sich an Sammlerinnen und Sammler deutscher Heftromane (auch bekannt als Groschenromane oder Groschenhefte) und bietet eine zentrale Plattform zur Katalogisierung, Präsentation und zum Tausch von Romanheften.

Das Projekt adressiert eine bestehende Lücke im Markt: Während es generische Bücherverwaltungen und allgemeine Sammlersoftware gibt, existiert keine spezialisierte Lösung für die Bedürfnisse von Heftroman-Sammlern. Diese haben besondere Anforderungen hinsichtlich Zustandsbewertung, serienbasierter Verwaltung und dem Fehlen von ISBN-Nummern.

### 1.1 Steckbrief

| Eigenschaft | Wert |
|---|---|
| **Projektname** | LILLY – Listing Inventory for Lovely Little Yellowbacks |
| **Projekttyp** | Open-Source-Webanwendung (PWA) |
| **Lizenz** | Open Source (MIT oder GPL – finale Wahl offen) |
| **Zielgruppe** | Sammler/innen deutschsprachiger Heftromane |
| **Initiale Serien** | Maddrax – Die dunkle Zukunft der Erde; Geisterjäger John Sinclair |
| **Datenquellen** | Maddraxikon (de.maddraxikon.com); Gruselroman-Wiki (gruselroman-wiki.de) |
| **Technologie** | Progressive Web App (PWA), self-hosted |
| **Autor / Initiator** | Holger Ehrmann |

---

## 2. Rahmenbedingungen

### 2.1 Technische Rahmenbedingungen

- **Architektur:** Progressive Web App (PWA) mit einer einzigen Codebasis für alle Plattformen (Windows, macOS, Linux, iOS, Android).
- **Hosting:** Self-Hosting auf eigenem Server bzw. VPS. Kein Vendor-Lock-in durch Cloud-Anbieter.
- **Offline-Fähigkeit:** Grundfunktionen der Sammlungsverwaltung müssen offline verfügbar sein (Service Worker, lokaler Cache). Synchronisation bei Wiederherstellung der Netzwerkverbindung.
- **Internationalisierung (i18n):** Initiale Oberflächensprache ist Deutsch. Die Architektur muss von Beginn an i18n-fähig sein, um spätere Übersetzungen zu ermöglichen.
- **Responsive Design:** Die Anwendung muss auf allen Bildschirmgrößen (Smartphone, Tablet, Desktop) vollständig nutzbar sein. Mobile-First-Ansatz empfohlen.

### 2.2 Organisatorische Rahmenbedingungen

- **Open Source:** Der gesamte Quellcode wird unter einer Open-Source-Lizenz (MIT oder GPL) veröffentlicht. Community-Beiträge sind erwünscht.
- **Kein kommerzielles Geschäftsmodell:** LILLY ist ein Community-Projekt. Es wird kein Unternehmen gegründet. Es gibt keinen Verkauf, keine Provisionen, keine Werbung.
- **Datenschutz:** Konformität mit der DSGVO ist erforderlich, insbesondere hinsichtlich Nutzerdaten, Profil-Öffentlichkeit und Nachrichten.
- **Urheberrecht:** Cover-Bilder aus den Fan-Wikis dürfen nur unter Beachtung der jeweiligen Lizenzbedingungen der Wikis eingebunden werden. Nutzer-Uploads unterliegen der Verantwortung der Nutzer.

---

## 3. Funktionale Anforderungen

*Legende Prioritäten: **MUSS** = zwingend für Release 1.0; **SOLL** = geplant für Release 1.0; **KANN** = wünschenswert, ggf. spätere Version.*

*Legende Kategorien: SV = Sammlungsverwaltung; DI = Datenimport; TA = Tausch; CO = Community; BN = Benutzerverwaltung; FO = Foto/Medien.*

### 3.1 Sammlungsverwaltung

| ID | Beschreibung | Prio | Kat. |
|---|---|---|---|
| SV-001 | Nutzer können aus den verfügbaren Heftroman-Serien einzelne Hefte zu ihrer persönlichen Sammlung hinzufügen. | MUSS | SV |
| SV-002 | Zu jedem Heft in der Sammlung kann der Zustand gemäß der Heftroman-Zustandsskala (Z0 bis Z5) angegeben werden: Z0 = neuwertig/ungelesen, Z1 = sehr gut, Z2 = gut, Z3 = akzeptabel, Z4 = mängelbehaftet, Z5 = schlecht/defekt. | MUSS | SV |
| SV-003 | Nutzer können zu jedem Heft in ihrer Sammlung persönliche Notizen hinterlegen (z. B. Besonderheiten, Auflagenhinweise, Erinnerungen). | SOLL | SV |
| SV-004 | Die Sammlung kann nach Serie, Heftnummer, Zustand, Titel und Autor gefiltert und sortiert werden. | MUSS | SV |
| SV-005 | Nutzer können Hefte als „Vorhanden", „Doppelt/Tauschbar" oder „Gesucht" markieren. | MUSS | SV |
| SV-006 | Eine Übersichtsansicht zeigt den Sammlungsfortschritt pro Serie als Balken oder Prozentwert an (z. B. „437 von 620 Heften – 70,5 %"). | MUSS | SV |
| SV-007 | Eine Rasteransicht zeigt alle Hefte einer Serie als Grid mit farblicher Kennzeichnung des Besitzstatus (vorhanden / fehlend / doppelt). | SOLL | SV |
| SV-008 | Import und Export der Sammlungsdaten im CSV- und JSON-Format. | SOLL | SV |
| SV-009 | Nutzer können mehrere Auflagen desselben Heftes getrennt erfassen. | KANN | SV |

### 3.2 Seriendaten und Datenimport

| ID | Beschreibung | Prio | Kat. |
|---|---|---|---|
| DI-001 | Zum Release werden die Stammdaten der Serien „Maddrax – Die dunkle Zukunft der Erde" und „Geisterjäger John Sinclair" vollständig importiert (Heftnummer, Titel, Autor, Ersterscheinungsdatum). | MUSS | DI |
| DI-002 | Datenquelle für Maddrax ist das Maddraxikon (de.maddraxikon.com). Datenquelle für John Sinclair ist das Gruselroman-Wiki (gruselroman-wiki.de). | MUSS | DI |
| DI-003 | Der Datenimport erfolgt als einmaliger initialer Import mit anschließendem regelmäßigem Sync per Cronjob (z. B. wöchentlich), um neue Hefte automatisch zu erfassen. | MUSS | DI |
| DI-004 | Cover-Bilder der Hefte werden aus den Wiki-Quellen als Referenzbilder importiert, sofern lizenzrechtlich zulässig. | SOLL | DI |
| DI-005 | Das Datenimport-System muss modular aufgebaut sein, sodass weitere Heftroman-Serien und Datenquellen mit vertretbarem Aufwand hinzugefügt werden können (z. B. Perry Rhodan via Perrypedia). | MUSS | DI |
| DI-006 | Zu jeder Serie werden Metadaten gespeichert: Serienname, Verlag, Genre, Erscheinungsrhythmus, Gesamtzahl der Hefte (soweit bekannt), Status (laufend/abgeschlossen). | SOLL | DI |

### 3.3 Tausch-System

| ID | Beschreibung | Prio | Kat. |
|---|---|---|---|
| TA-001 | Nutzer können Hefte, die sie als „Doppelt/Tauschbar" markiert haben, für den Tausch anbieten. | MUSS | TA |
| TA-002 | Nutzer können eine „Suche"-Liste führen mit Heften, die sie suchen (automatisch ableitbar aus der Fehl-Liste). | MUSS | TA |
| TA-003 | Das System bietet semi-automatisches Matching: Wenn Nutzer A ein Heft anbietet, das Nutzer B sucht (und umgekehrt), werden beide über einen potenziellen Tausch benachrichtigt. | MUSS | TA |
| TA-004 | Tauschpartner können sich über ein internes Nachrichtensystem kontaktieren, um Details (Versand, Zustand etc.) zu klären. | MUSS | TA |
| TA-005 | Ein Tausch kann von beiden Seiten als „Abgeschlossen" markiert werden. Abgeschlossene Tausche aktualisieren automatisch die jeweiligen Sammlungen. | SOLL | TA |
| TA-006 | LILLY ist ausdrücklich eine Tausch-Plattform. Es gibt keine Kauf-/Verkaufsfunktion, keine Preisangaben und kein Zahlungssystem. Der finanzielle Aspekt wird bewusst ausgeklammert. | MUSS | TA |
| TA-007 | Nutzer können ein optionales Bewertungssystem für Tauschpartner nutzen (z. B. Zuverlässigkeit, Zustandsangaben korrekt). | KANN | TA |

### 3.4 Benutzerverwaltung und Authentifizierung

| ID | Beschreibung | Prio | Kat. |
|---|---|---|---|
| BN-001 | Registrierung ist möglich über klassische E-Mail/Passwort-Kombination. | MUSS | BN |
| BN-002 | Registrierung ist alternativ möglich über OAuth-Anbieter (mindestens Google und GitHub). | MUSS | BN |
| BN-003 | Nutzer können ein Profil mit Anzeigename, Avatar und optionaler Standortangabe (für Tausch-Nähe) anlegen. | MUSS | BN |
| BN-004 | Nutzer können wählen, ob ihr Profil und ihre Sammlung öffentlich oder privat sind. | MUSS | BN |
| BN-005 | Passwort-Reset per E-Mail muss implementiert sein. | MUSS | BN |
| BN-006 | Zwei-Faktor-Authentifizierung (2FA) als optionale Sicherheitserweiterung. | KANN | BN |
| BN-007 | Nutzer können ihren Account und alle zugeordneten Daten vollständig löschen (Recht auf Löschung gemäß DSGVO). | MUSS | BN |

### 3.5 Foto- und Medienverwaltung

| ID | Beschreibung | Prio | Kat. |
|---|---|---|---|
| FO-001 | Zu jedem Heft in der Sammlung kann das Referenz-Cover aus der Wiki-Datenquelle angezeigt werden. | MUSS | FO |
| FO-002 | Nutzer können eigene Fotos zu ihren Heften hochladen (z. B. Zustandsdokumentation, Rückseite, Besonderheiten). Upload per Kamera (mobil) oder Dateiauswahl (Desktop). | MUSS | FO |
| FO-003 | Hochgeladene Fotos werden serverseitig gespeichert und sind an das jeweilige Heft in der Nutzer-Sammlung gebunden. | MUSS | FO |
| FO-004 | Fotos werden automatisch komprimiert und in einheitlicher Größe gespeichert, um Speicherplatz zu sparen. | SOLL | FO |
| FO-005 | Pro Heft können mindestens 4 eigene Fotos hochgeladen werden. | SOLL | FO |

### 3.6 Community-Features

| ID | Beschreibung | Prio | Kat. |
|---|---|---|---|
| CO-001 | Öffentliche Sammler-Profile zeigen Statistiken an: Sammlungsfortschritt pro Serie (absolut und prozentual), Gesamtzahl der Hefte, Mitglied seit. | MUSS | CO |
| CO-002 | Nutzer können Kommentare und Bewertungen (z. B. 1–5 Sterne) zu einzelnen Heften abgeben. | SOLL | CO |
| CO-003 | Wunschlisten (gesuchte Hefte) und Tausch-Listen (doppelte Hefte) können öffentlich geteilt werden, optional als direkter Link. | MUSS | CO |
| CO-004 | Nutzer können die öffentlichen Sammlungen anderer Nutzer durchsuchen und einsehen. | SOLL | CO |
| CO-005 | Eine übergreifende Statistik-Seite zeigt aggregierte Daten: beliebteste Serien, aktivste Tauscher, am häufigsten gesuchte Hefte. | KANN | CO |

---

## 4. Nicht-funktionale Anforderungen

### 4.1 Performance

- Die Anwendung muss auf mobilen Geräten mit durchschnittlicher Internetverbindung innerhalb von 3 Sekunden vollständig geladen sein (First Contentful Paint).
- Suche und Filterung innerhalb der eigenen Sammlung muss auch bei > 5.000 Einträgen performant sein (< 500 ms Antwortzeit).
- Der Lighthouse Performance Score der PWA sollte mindestens 90 betragen.

### 4.2 Verfügbarkeit und Betrieb

- Self-Hosting auf einem einzelnen VPS mit Docker-Containerisierung.
- Deployment via Docker Compose oder vergleichbar. Einfache Installation mit dokumentiertem Setup-Prozess.
- Automatisierte Backups der Datenbank und der Nutzer-Uploads.
- Monitoring-Endpunkt (Health Check) für den Betrieb.

### 4.3 Sicherheit

- HTTPS-Pflicht für alle Verbindungen.
- Passwort-Hashing mit bcrypt oder Argon2.
- Schutz gegen gängige Angriffsvektoren: SQL Injection, XSS, CSRF.
- Rate Limiting für Authentifizierungs-Endpunkte und API-Zugriffe.
- Upload-Validierung: nur erlaubte Dateitypen (JPEG, PNG, WebP), maximale Dateigröße pro Upload.

### 4.4 Datenschutz (DSGVO)

- Impressum und Datenschutzerklärung müssen vorhanden sein.
- Nutzer müssen der Datenverarbeitung bei Registrierung zustimmen.
- Recht auf Auskunft: Nutzer können einen Export aller über sie gespeicherten Daten anfordern.
- Recht auf Löschung: Vollständige Accountlöschung inkl. aller zugeordneten Daten.
- Keine Weitergabe personenbezogener Daten an Dritte.

### 4.5 Wartbarkeit und Erweiterbarkeit

- Modularer Aufbau: Neue Heftroman-Serien müssen durch Hinzufügen eines Datenimport-Moduls integrierbar sein, ohne Kerncode zu ändern.
- API-First-Ansatz: Alle Funktionen sind über eine REST- oder GraphQL-API erreichbar. Die PWA nutzt dieselbe API.
- Automatisierte Tests (Unit, Integration) mit einer Mindest-Testabdeckung von 70 %.
- CI/CD-Pipeline für automatisiertes Testing und Deployment.
- Dokumentation: README, API-Dokumentation, Entwickler-Handbuch für Community-Beiträge.

---

## 5. Zustandsbewertungsskala für Heftromane

Die folgende Skala ist der etablierte Standard in der Heftroman-Sammlerszene und wird in LILLY als Pflichtfeld bei der Erfassung verwendet:

| Stufe | Bezeichnung | Beschreibung |
|---|---|---|
| **Z0** | Neuwertig | Ungelesen, keine Gebrauchsspuren, Zustand wie am Kiosk. Makellos. |
| **Z1** | Sehr gut | Minimal gelesen, kaum sichtbare Gebrauchsspuren. Keine Knicke, Risse oder Flecken. |
| **Z2** | Gut | Gelesen, leichte Gebrauchsspuren. Kleinere Knicke an Ecken möglich, keine Risse oder Flecken. |
| **Z3** | Akzeptabel | Deutliche Gebrauchsspuren. Knicke, leichte Verfärbungen, eventuell kleiner Stempel oder Name. Vollständig und lesbar. |
| **Z4** | Mängelbehaftet | Starke Gebrauchsspuren. Risse, Flecken, lose Seiten, fehlende Beilagen. Noch vollständig lesbar. |
| **Z5** | Schlecht / Defekt | Schwere Schäden. Fehlende Seiten, starke Verschmutzung, Wasserschäden. Nur als Platzhalter oder Lesekopie geeignet. |

---

## 6. Datenmodell (Übersicht)

Die folgenden Kernentitäten bilden das Datenmodell von LILLY. Das detaillierte Datenbankschema wird im technischen Designdokument spezifiziert.

### 6.1 Kernentitäten

- **Serie (Series):** Serienname, Verlag, Genre, Erscheinungsrhythmus, Status, Datenquelle-URL, Gesamtzahl Hefte.
- **Heft (Issue):** Heftnummer, Titel, Autor(en), Ersterscheinungsdatum, Serienzugehörigkeit, Cover-URL, Zyklus/Handlungsabschnitt.
- **Nutzer (User):** Anzeigename, E-Mail (verschlüsselt), Passwort-Hash, OAuth-Referenzen, Avatar, Standort (optional), Profil-Sichtbarkeit.
- **Sammlungseintrag (CollectionEntry):** Nutzer-Referenz, Heft-Referenz, Zustand (Z0–Z5), Status (vorhanden/doppelt/gesucht), Notizen, eigene Fotos.
- **Tausch (Trade):** Anbieter, Nachfrager, angebotene Hefte, gesuchte Hefte, Status (offen/akzeptiert/abgeschlossen/abgebrochen), Nachrichten-Thread.
- **Nachricht (Message):** Absender, Empfänger, Tausch-Referenz (optional), Inhalt, Zeitstempel, Gelesen-Status.
- **Kommentar (Comment):** Nutzer-Referenz, Heft-Referenz, Text, Bewertung (1–5 Sterne), Zeitstempel.

---

## 7. Abgrenzungen

Folgende Funktionen sind bewusst nicht Teil von LILLY:

- **Kein Kauf-/Verkaufssystem:** LILLY ist ausschließlich eine Tauschplattform. Verkäufe können allenfalls informell über das Nachrichtensystem zwischen Nutzern vereinbart werden, werden aber nicht durch die Plattform unterstützt oder gefördert.
- **Kein Zahlungssystem:** Keine Integration von Zahlungsdienstleistern, keine Preisangaben, keine Provisionen.
- **Kein Forum oder Chat:** Die Kommunikation beschränkt sich auf das interne Nachrichtensystem im Kontext von Tauschvorgängen und Direktnachrichten. Ein vollwertiges Forum ist nicht vorgesehen.
- **Keine eigene Content-Erstellung:** LILLY erstellt keine eigenen redaktionellen Inhalte zu den Serien. Die Stammdaten stammen aus externen Fan-Wikis.
- **Keine E-Book- oder Digital-Reader-Funktion:** LILLY verwaltet physische Heftromane, keine digitalen Inhalte.

---

## 8. Roadmap (Entwurf)

### 8.1 Phase 1 – MVP (Release 1.0)

- Sammlungsverwaltung für Maddrax und John Sinclair
- Datenimport aus Maddraxikon und Gruselroman-Wiki
- Zustandsbewertung (Z0–Z5)
- Nutzerregistrierung (E-Mail + OAuth)
- Tausch-Matching und Nachrichtensystem
- Öffentliche Profile und Statistiken
- Foto-Upload für eigene Hefte
- PWA mit Offline-Grundfunktionalität

### 8.2 Phase 2 – Erweiterung

- Weitere Serien (z. B. Perry Rhodan, Professor Zamorra, Ren Dhark)
- Erweiterte Tausch-Features (Bewertungssystem für Tauschpartner)
- Aggregierte Community-Statistiken
- Benachrichtigungssystem (E-Mail, Push)
- Mehrsprachigkeit (Englisch als zweite Sprache)

### 8.3 Phase 3 – Vision

- Ringtausch-Algorithmus (A→B→C→A)
- Barcode-/Cover-Scan zur schnellen Erfassung
- Integration weiterer Datenquellen und internationaler Heftroman-Serien
- Sammler-Events und Tauschbörsen-Kalender

---

## 9. Glossar

| Begriff | Erläuterung |
|---|---|
| **Heftroman** | Gehefteter Roman im Format DIN C5 bis A5, erscheint periodisch im Zeitschriftenhandel. Auch: Groschenroman, Groschenheft. |
| **Yellowback** | Historischer englischer Begriff für billige, in gelbe Umschläge gebundene Trivialromane des 19. Jahrhunderts. Namensgebend für LILLY. |
| **Zustandsskala (Z0–Z5)** | Standardisierte Bewertungsskala für den physischen Zustand von Romanheften in der deutschen Sammlerszene. |
| **PWA** | Progressive Web App. Webanwendung, die wie eine native App installiert und teilweise offline genutzt werden kann. |
| **Fan-Wiki** | Von Fans betriebene, auf MediaWiki basierende Wissensdatenbank zu einer Heftroman-Serie (z. B. Maddraxikon, Perrypedia). |
| **Matching** | Automatischer Abgleich von Angebots- und Suchlisten verschiedener Nutzer zur Identifikation potenzieller Tauschpartner. |
| **i18n** | Internationalisierung (Internationalization). Architektonische Vorbereitung einer Anwendung für mehrere Sprachen. |
| **Self-Hosting** | Betrieb der Anwendung auf eigener Infrastruktur (eigener Server / VPS) statt bei einem Cloud-Anbieter. |

---

*Ende des Anforderungskatalogs*