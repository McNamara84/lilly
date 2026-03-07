# LILLY – Listing Inventory for Lovely Little Yellowbacks

## Architektur- und Designdokument

**Version 1.0** | Stand: 06. März 2026 | Basierend auf: Anforderungskatalog v1.0 | Autor: Holger Ehrmann

---

## Inhaltsverzeichnis

1. [Einleitung](#1-einleitung)
2. [Technologie-Stack](#2-technologie-stack)
3. [Systemarchitektur](#3-systemarchitektur)
4. [Datenbankschema](#4-datenbankschema)
5. [API-Design](#5-api-design)
6. [Datenimport-Architektur](#6-datenimport-architektur)
7. [PWA und Offline-Strategie](#7-pwa-und-offline-strategie)
8. [Deployment](#8-deployment)
9. [Projektstruktur](#9-projektstruktur)
10. [Sicherheitsarchitektur](#10-sicherheitsarchitektur)

---

## 1. Einleitung

Dieses Dokument beschreibt die technische Architektur und das Design der Anwendung LILLY (Listing Inventory for Lovely Little Yellowbacks). Es baut auf dem Anforderungskatalog v1.0 auf und überführt die dort definierten funktionalen und nicht-funktionalen Anforderungen in konkrete technische Entscheidungen.

Ziel ist es, eine klare technische Grundlage für die Implementierung zu schaffen, die sowohl für den Hauptentwickler als auch für zukünftige Open-Source-Beitragende verständlich und nachvollziehbar ist.

### 1.1 Design-Prinzipien

- **Performance first:** Der gewählte Tech-Stack (Svelte 5, Rust, MariaDB) priorisiert Geschwindigkeit und geringen Ressourcenverbrauch auf allen Ebenen.
- **Self-Hosting-Optimiert:** Alle Komponenten laufen in Docker-Containern auf einem einzelnen VPS. Keine Abhängigkeiten von Cloud-Diensten.
- **Modularer Datenimport:** Neue Heftroman-Serien können durch Hinzufügen eines Import-Moduls integriert werden, ohne Kerncode zu ändern.
- **API-First:** Das Frontend kommuniziert ausschließlich über eine dokumentierte REST-API mit dem Backend. Drittanbieter-Clients sind dadurch möglich.
- **Offline-fähig:** Die PWA-Architektur ermöglicht Grundfunktionen ohne Internetverbindung.

---

## 2. Technologie-Stack

### 2.1 Übersicht

| Komponente | Technologie | Details |
|---|---|---|
| **Frontend** | Svelte 5 / SvelteKit | Kompiliert zu minimalem JS, Runes-Reaktivität, SSR + CSR, integrierte PWA-Unterstützung via Vite-Plugin |
| **UI-Framework** | Skeleton UI + Tailwind CSS | Svelte-native Komponentenbibliothek, Tailwind für Utility-First-Styling, Dark/Light Mode |
| **Backend / API** | Rust + Axum | Async HTTP-Framework auf Basis von Tokio, Tower-Middleware, modularer Router |
| **Datenbank** | MariaDB 11.x | Relationale Datenbank, InnoDB-Engine, Volltextsuche, bewährte MySQL-Kompatibilität |
| **DB-Zugriff** | SQLx | Compile-time verified SQL-Queries, async, kein ORM-Overhead, Migrations-System |
| **Authentifizierung** | Eigenbau: JWT + argon2id | Access/Refresh-Token-Paar, argon2id für Passwort-Hashing, OAuth2-Client für Google/GitHub |
| **API-Spezifikation** | OpenAPI 3.1 / Swagger | Generiert via utoipa-Crate (Rust), Swagger-UI als Dev-Tool |
| **Dateispeicher** | Lokales Dateisystem | Strukturiertes Verzeichnis, Caddy Static Serving, automatische Bildkompression |
| **Reverse Proxy** | Caddy v2 | Automatisches HTTPS via Let's Encrypt, minimale Konfiguration, HTTP/2 + HTTP/3 |
| **Containerisierung** | Docker + Docker Compose | Multi-Container-Setup, isolierte Services, einfaches Deployment |
| **Wiki-Importer** | Rust (reqwest + scraper) | CLI-Tool und Cronjob-fähig, MediaWiki-API + HTML-Parsing, modulare Adapter |
| **i18n** | Paraglide.js (SvelteKit) | Typsichere Übersetzungen, Tree-Shaking, initiale Sprache Deutsch |

### 2.2 Begründung der Kernentscheidungen

**Svelte 5 / SvelteKit als Frontend**

Svelte 5 kompiliert Komponenten zur Build-Zeit zu optimiertem JavaScript, wodurch kein Framework-Runtime-Overhead im Browser entsteht. Das neue Runes-System bietet fein-granulare Reaktivität. SvelteKit liefert Routing, SSR, Service Worker und Build-Pipeline aus einer Hand. In Kombination mit dem Vite-PWA-Plugin entsteht eine installierbare, offline-fähige Anwendung mit minimalem Konfigurationsaufwand. Skeleton UI bietet als Svelte-native Komponentenbibliothek hochwertige, barrierefreie UI-Komponenten auf Tailwind-Basis.

**Rust mit Axum als Backend**

Rust bietet Memory Safety ohne Garbage Collector und ermöglicht extrem ressourceneffiziente Server-Anwendungen – ideal für Self-Hosting auf einem einzelnen VPS. Axum ist das modernste async Web-Framework im Rust-Ökosystem, aufgebaut auf dem bewährten Tokio-Runtime und dem Tower-Middleware-Stack. SQLx als Datenbankschicht prüft SQL-Queries bereits zur Compile-Zeit gegen das tatsächliche Datenbankschema, was eine ganze Klasse von Laufzeitfehlern eliminiert.

**MariaDB als Datenbank**

MariaDB ist ein ausgereiftes, performantes RDBMS mit vollständiger MySQL-Kompatibilität. Die relationalen Datenstrukturen von LILLY (Serien, Hefte, Sammlungen, Tausche) profitieren von referentieller Integrität und JOIN-Operationen. MariaDB bietet zudem integrierte Volltextsuche, die für die Heft- und Seriensuche genutzt werden kann. Die breite Hosting-Kompatibilität erleichtert Self-Hosting und Community-Deployments.

**Caddy als Reverse Proxy**

Caddy v2 bietet automatisches HTTPS über integriertes ACME-Protokoll (Let's Encrypt) mit minimalem Konfigurationsaufwand. Ein typisches Caddyfile für LILLY umfasst weniger als 10 Zeilen. Caddy unterstützt HTTP/2 und HTTP/3 out-of-the-box und dient gleichzeitig als Static File Server für die hochgeladenen Fotos.

---

## 3. Systemarchitektur

### 3.1 Komponentendiagramm

```
┌──────────────────────────────────────────────────────────────┐
│                      Docker Host (VPS)                        │
│                                                              │
│  ┌────────────┐    ┌────────────┐    ┌──────────────┐        │
│  │   Caddy    │    │  SvelteKit │    │  Rust / Axum │        │
│  │  (Reverse  │────│  (Frontend │    │   (Backend)  │        │
│  │   Proxy)   │    │   SSR/PWA) │────│  REST API    │        │
│  └────────────┘    └────────────┘    └──────┬───────┘        │
│       │                                     │                │
│       │  Static Files          ┌────────────┘                │
│       └────────────────────────┤   MariaDB    │              │
│  ┌────────────┐                │   11.x       │              │
│  │   /media   │                └──────────────┘              │
│  │  (Volume)  │                                              │
│  └────────────┘  ┌────────────────────────┐                  │
│                  │   Wiki-Importer (Cron)  │                  │
│                  │   Rust CLI: reqwest +   │                  │
│                  │   scraper → MariaDB     │                  │
│                  └────────────────────────┘                   │
└──────────────────────────────────────────────────────────────┘
```

### 3.2 Container-Übersicht (Docker Compose)

Das System besteht aus fünf Docker-Containern, orchestriert via Docker Compose:

| Container | Image | Port (intern) | Aufgabe |
|---|---|---|---|
| `caddy` | `caddy:2-alpine` | 80, 443 → extern | HTTPS-Terminierung, Reverse Proxy, Static File Serving für /media |
| `frontend` | `node:22-alpine` + Build | 3000 (intern) | SvelteKit SSR-Server, liefert PWA-Shell und pre-rendered Pages |
| `backend` | `rust:slim` + Build | 8080 (intern) | REST API (Axum), Authentifizierung, Business-Logik, Bildverarbeitung |
| `db` | `mariadb:11` | 3306 (intern) | Persistente Datenhaltung, Volltextindex |
| `importer` | Rust CLI (eigener Build) | – | Cronjob-basierter Wiki-Datenimport, schreibt direkt in MariaDB |

### 3.3 Request-Flow

Der typische Ablauf einer Nutzeranfrage:

1. **Client → Caddy:** Alle eingehenden Requests landen bei Caddy (Port 443). Caddy terminiert TLS und routet basierend auf dem Pfad.
2. **Caddy → Frontend:** Seiten-Requests (HTML, JS, CSS) werden an den SvelteKit-Server (Port 3000) weitergeleitet. SvelteKit liefert SSR-gerenderte Seiten oder die PWA-Shell.
3. **Caddy → Backend:** API-Requests unter `/api/*` werden direkt an den Axum-Server (Port 8080) geroutet.
4. **Caddy → Dateisystem:** Requests unter `/media/*` werden direkt von Caddy als statische Dateien aus dem gemounteten Volume serviert (Fotos, Cover).
5. **Backend → MariaDB:** Der Axum-Server kommuniziert über SQLx mit MariaDB für alle Datenoperationen.

---

## 4. Datenbankschema

Das folgende Schema definiert die Kernentitäten und ihre Beziehungen. Alle Tabellen verwenden InnoDB als Storage Engine und UTF-8mb4 als Zeichensatz.

### 4.1 Tabelle: `series`

| Spalte | Typ | Constraint | Beschreibung |
|---|---|---|---|
| `id` | INT UNSIGNED | PK, AUTO_INC | Primärschlüssel |
| `name` | VARCHAR(255) | NOT NULL, UQ | Serienname (z. B. „Maddrax – Die dunkle Zukunft der Erde") |
| `slug` | VARCHAR(255) | NOT NULL, UQ | URL-freundlicher Bezeichner (z. B. „maddrax") |
| `publisher` | VARCHAR(255) | NULL | Verlag |
| `genre` | VARCHAR(100) | NULL | Genre (Science-Fiction, Horror, Western etc.) |
| `frequency` | VARCHAR(50) | NULL | Erscheinungsrhythmus (wöchentlich, 14-tägig etc.) |
| `total_issues` | INT UNSIGNED | NULL | Aktuelle Gesamtzahl Hefte (NULL bei laufenden Serien) |
| `status` | ENUM | NOT NULL | 'running' \| 'completed' \| 'cancelled' |
| `source_url` | VARCHAR(500) | NULL | URL der Datenquelle (Wiki) |
| `created_at` | TIMESTAMP | NOT NULL | Erstellungszeitpunkt |
| `updated_at` | TIMESTAMP | NOT NULL | Letzter Sync-Zeitpunkt |

### 4.2 Tabelle: `issues`

| Spalte | Typ | Constraint | Beschreibung |
|---|---|---|---|
| `id` | INT UNSIGNED | PK, AUTO_INC | Primärschlüssel |
| `series_id` | INT UNSIGNED | FK, NOT NULL | Fremdschlüssel auf series.id |
| `issue_number` | INT UNSIGNED | NOT NULL | Heftnummer innerhalb der Serie |
| `title` | VARCHAR(500) | NOT NULL | Titel des Heftes |
| `author` | VARCHAR(500) | NULL | Autor(en), kommasepariert |
| `published_at` | DATE | NULL | Ersterscheinungsdatum |
| `cycle` | VARCHAR(255) | NULL | Zyklus / Handlungsabschnitt |
| `cover_url` | VARCHAR(500) | NULL | Pfad zum Cover-Bild (lokal oder Wiki-URL) |
| `source_wiki_url` | VARCHAR(500) | NULL | Link zum Wiki-Eintrag des Heftes |
| `created_at` | TIMESTAMP | NOT NULL | Import-Zeitpunkt |

*Unique Index: `(series_id, issue_number)` – verhindert doppelte Hefteinträge pro Serie.*

### 4.3 Tabelle: `users`

| Spalte | Typ | Constraint | Beschreibung |
|---|---|---|---|
| `id` | INT UNSIGNED | PK, AUTO_INC | Primärschlüssel |
| `email` | VARCHAR(255) | NOT NULL, UQ | E-Mail-Adresse (verschlüsselt gespeichert) |
| `password_hash` | VARCHAR(255) | NULL | argon2id-Hash (NULL bei reinem OAuth-Login) |
| `display_name` | VARCHAR(100) | NOT NULL | Anzeigename / Sammlername |
| `avatar_path` | VARCHAR(500) | NULL | Pfad zum Avatar-Bild |
| `location` | VARCHAR(255) | NULL | Standort (freiwillig, für Tausch-Nähe) |
| `profile_public` | BOOLEAN | NOT NULL, DEF 0 | Profil öffentlich sichtbar? |
| `oauth_provider` | VARCHAR(50) | NULL | 'google' \| 'github' \| NULL |
| `oauth_id` | VARCHAR(255) | NULL | Externe OAuth-ID |
| `created_at` | TIMESTAMP | NOT NULL | Registrierungszeitpunkt |

### 4.4 Tabelle: `collection_entries`

| Spalte | Typ | Constraint | Beschreibung |
|---|---|---|---|
| `id` | INT UNSIGNED | PK, AUTO_INC | Primärschlüssel |
| `user_id` | INT UNSIGNED | FK, NOT NULL | Fremdschlüssel auf users.id (ON DELETE CASCADE) |
| `issue_id` | INT UNSIGNED | FK, NOT NULL | Fremdschlüssel auf issues.id |
| `condition_grade` | ENUM | NOT NULL | 'Z0' \| 'Z1' \| 'Z2' \| 'Z3' \| 'Z4' \| 'Z5' |
| `status` | ENUM | NOT NULL | 'owned' \| 'duplicate' \| 'wanted' |
| `notes` | TEXT | NULL | Persönliche Notizen |
| `created_at` | TIMESTAMP | NOT NULL | Zeitpunkt der Erfassung |
| `updated_at` | TIMESTAMP | NOT NULL | Letzte Änderung |

*Unique Index: `(user_id, issue_id, status)` – ein Nutzer kann dasselbe Heft als „owned" und „duplicate" haben, aber nicht doppelt im selben Status.*

### 4.5 Tabelle: `collection_photos`

| Spalte | Typ | Constraint | Beschreibung |
|---|---|---|---|
| `id` | INT UNSIGNED | PK, AUTO_INC | Primärschlüssel |
| `entry_id` | INT UNSIGNED | FK, NOT NULL | Fremdschlüssel auf collection_entries.id (ON DELETE CASCADE) |
| `file_path` | VARCHAR(500) | NOT NULL | Relativer Pfad im /media-Volume |
| `sort_order` | TINYINT | NOT NULL, DEF 0 | Sortierreihenfolge der Fotos |
| `created_at` | TIMESTAMP | NOT NULL | Upload-Zeitpunkt |

### 4.6 Tabellen: trades, messages, comments

Die verbleibenden Tabellen folgen demselben Muster. Hier eine kompakte Übersicht der Kernfelder:

**trades**
- `id`, `initiator_id` (FK users), `responder_id` (FK users), `status` ENUM('proposed', 'accepted', 'completed', 'cancelled'), `created_at`, `updated_at`

**trade_items**
- `id`, `trade_id` (FK trades), `entry_id` (FK collection_entries), `direction` ENUM('offered', 'requested')

**messages**
- `id`, `sender_id` (FK users), `recipient_id` (FK users), `trade_id` (FK trades, NULL), `content` TEXT, `is_read` BOOLEAN, `created_at`

**comments**
- `id`, `user_id` (FK users), `issue_id` (FK issues), `text` TEXT, `rating` TINYINT (1–5), `created_at`, `updated_at`

---

## 5. API-Design

Alle Endpunkte sind unter dem Präfix `/api/v1` erreichbar. Authentifizierte Endpunkte erfordern einen gültigen JWT im Authorization-Header (Bearer Token).

### 5.1 Endpunkt-Übersicht

| Methode | Pfad | Auth | Beschreibung |
|---|---|---|---|
| **POST** | `/api/v1/auth/register` | Nein | Registrierung (E-Mail/Passwort) |
| **POST** | `/api/v1/auth/login` | Nein | Login → Access + Refresh Token |
| **POST** | `/api/v1/auth/oauth/{provider}` | Nein | OAuth-Login (Google/GitHub) |
| **POST** | `/api/v1/auth/refresh` | Refresh | Token-Erneuerung |
| **GET** | `/api/v1/series` | Nein | Alle Serien auflisten |
| **GET** | `/api/v1/series/{slug}/issues` | Nein | Alle Hefte einer Serie (paginiert) |
| **GET** | `/api/v1/issues/{id}` | Nein | Heft-Details + Community-Kommentare |
| **GET** | `/api/v1/me/collection` | Ja | Eigene Sammlung (Filter, Paginierung) |
| **POST** | `/api/v1/me/collection` | Ja | Heft zur Sammlung hinzufügen |
| **PATCH** | `/api/v1/me/collection/{id}` | Ja | Eintrag ändern (Zustand, Status, Notizen) |
| **DELETE** | `/api/v1/me/collection/{id}` | Ja | Eintrag entfernen |
| **POST** | `/api/v1/me/collection/{id}/photos` | Ja | Foto hochladen (multipart/form-data) |
| **GET** | `/api/v1/me/trades` | Ja | Eigene Tauschvorgänge |
| **GET** | `/api/v1/me/matches` | Ja | Potenzielle Tauschpartner (Matching) |
| **POST** | `/api/v1/trades` | Ja | Tausch vorschlagen |
| **PATCH** | `/api/v1/trades/{id}` | Ja | Tausch-Status ändern |
| **GET** | `/api/v1/me/messages` | Ja | Nachrichten-Übersicht |
| **POST** | `/api/v1/messages` | Ja | Nachricht senden |
| **GET** | `/api/v1/me/collection/stats` | Ja | Sammlungsstatistiken (Gesamt, pro Serie, Doppelte, Gesuchte) |
| **GET** | `/api/v1/me/activity` | Ja | Letzte Aktivitäten des Nutzers (Timeline) |
| **GET** | `/api/v1/users/{name}/profile` | Nein | Öffentliches Profil + Statistiken |
| **GET** | `/api/v1/users/{name}/collection` | Nein | Öffentliche Sammlung (wenn freigegeben) |
| **GET** | `/api/v1/users` | Nein | Öffentliche Sammler-Liste (sortier-/filterbar) |
| **GET** | `/api/v1/issues/most-wanted` | Nein | Meistgesuchte Hefte plattformweit |

### 5.2 Abgeleiteter Status "Fehlend" (missing)

Der Status `missing` wird **nicht** in der Datenbank gespeichert. Er ist ein abgeleiteter (virtueller) Status, der sich aus der Differenz zwischen der Gesamtmenge der Hefte einer Serie (`issues`-Tabelle) und den Sammlungseinträgen des Nutzers (`collection_entries`-Tabelle) ergibt.

**Berechnung:** Ein Heft gilt als `missing`, wenn für die Kombination `(user_id, issue_id)` kein Eintrag in `collection_entries` existiert.

**API-Verhalten bei `GET /api/v1/me/collection?status=missing`:**

Wird der Filter `status=missing` angefragt, führt das Backend einen LEFT JOIN von `issues` (gefiltert nach Serie) auf `collection_entries` (gefiltert nach User) durch und liefert nur die Hefte zurück, die **keinen** zugehörigen Sammlungseintrag haben. Die Response enthält dann Issue-Objekte ohne `collection_entry`-Daten.

**API-Verhalten bei `GET /api/v1/series/{slug}/issues` (authentifiziert):**

Wenn ein authentifizierter Nutzer die Heftliste einer Serie abruft, reichert das Backend die Response optional mit dem Sammlungsstatus pro Heft an (owned/duplicate/wanted/null). Hefte mit `null`-Status gelten im Frontend als `missing`.

### 5.3 Authentifizierung

Die Authentifizierung basiert auf einem JWT-Paar:

- **Access Token:** Kurzlebig (15 Minuten), wird bei jedem API-Request im Authorization-Header mitgesendet. Enthält user_id und display_name als Claims.
- **Refresh Token:** Langlebig (30 Tage), wird als httpOnly-Cookie gespeichert. Dient ausschließlich zur Erneuerung des Access Tokens.
- **OAuth2 Flow:** Authorization Code Flow mit PKCE für Google und GitHub. Nach erfolgreicher OAuth-Authentifizierung wird ein lokaler JWT ausgestellt.
- **Passwort-Hashing:** argon2id mit empfohlenen Parametern (m=19456, t=2, p=1).

---

## 6. Datenimport-Architektur

### 6.1 Modulares Adapter-System

Der Wiki-Importer ist als Rust-CLI-Tool implementiert, das sowohl manuell als auch per Cronjob ausgeführt werden kann. Das Kernkonzept ist ein Adapter-Pattern:

- **Trait `WikiAdapter`:** Definiert die Schnittstelle, die jede Datenquelle implementieren muss: `fetch_series_metadata()`, `fetch_issue_list()`, `fetch_issue_details(number)`, `fetch_cover(number)`.
- **`MaddraxikonAdapter`:** Implementierung für de.maddraxikon.com. Nutzt eine Kombination aus MediaWiki-API (für strukturierte Daten) und HTML-Scraping (für Tabellen und Cover via reqwest + scraper).
- **`GruselromanWikiAdapter`:** Implementierung für gruselroman-wiki.de. Selbes Trait, angepasste Parsing-Logik.

### 6.2 Import-Ablauf

1. **Initialer Import:** Einmalige vollständige Erfassung aller Hefte einer Serie. Der Adapter iteriert über alle Heftnummern und schreibt Stammdaten + Cover in die Datenbank bzw. das Dateisystem.
2. **Inkrementeller Sync:** Wöchentlicher Cronjob prüft auf neue Hefte (Vergleich der höchsten Heftnummer in DB vs. Wiki) und importiert nur die Differenz.
3. **Cover-Download:** Cover-Bilder werden lokal im `/media/covers/{series_slug}/{number}.jpg`-Format gespeichert und automatisch auf eine einheitliche Größe skaliert.
4. **Logging:** Jeder Import-Lauf wird protokolliert (Anzahl neue Hefte, Fehler, Dauer), um Probleme früh zu erkennen.

### 6.3 Hinzufügen neuer Serien

Um eine neue Serie (z. B. Perry Rhodan via Perrypedia) zu integrieren, sind folgende Schritte erforderlich:

- Neuen Adapter implementieren, der den `WikiAdapter`-Trait erfüllt.
- Adapter in der CLI-Konfiguration registrieren.
- Initialen Import durchführen.
- Cronjob-Konfiguration um die neue Serie erweitern.

Es sind keine Änderungen am Backend, Frontend oder Datenbankschema notwendig.

---

## 7. PWA und Offline-Strategie

### 7.1 Service Worker

SvelteKit generiert in Kombination mit dem Vite-PWA-Plugin einen Service Worker, der folgende Caching-Strategien implementiert:

- **App Shell (Cache First):** HTML-Gerüst, JavaScript-Bundles, CSS und UI-Assets werden beim ersten Besuch gecacht und bei Updates im Hintergrund aktualisiert.
- **API-Daten (Network First):** Sammlungsdaten werden bevorzugt vom Server geladen. Bei fehlender Verbindung wird die letzte gecachte Version angezeigt.
- **Bilder (Stale While Revalidate):** Cover-Bilder und Fotos werden aus dem Cache serviert und im Hintergrund aktualisiert.

### 7.2 Offline-Fähigkeit

- **Lesen:** Die eigene Sammlung kann vollständig offline eingesehen werden (gecachte Daten + IndexedDB).
- **Schreiben:** Änderungen an der Sammlung (Zustand, Status, Notizen) werden lokal in einer Sync-Queue gespeichert und bei Wiederherstellung der Verbindung automatisch synchronisiert.
- **Fotos:** Foto-Uploads werden in der Queue gespeichert und bei nächster Gelegenheit hochgeladen.
- **Tausch:** Tausch-Funktionen erfordern eine aktive Internetverbindung.

---

## 8. Deployment

### 8.1 Docker Compose-Struktur

Das gesamte System wird über eine einzige `docker-compose.yml`-Datei definiert. Empfohlene Mindestanforderungen an den VPS: 2 vCPU, 4 GB RAM, 40 GB SSD.

### 8.2 Volumes

- **`db_data`:** Persistenter MariaDB-Speicher.
- **`media`:** Cover-Bilder und Nutzer-Uploads. Wird von Caddy als Static Files serviert und vom Backend beschrieben.
- **`caddy_data`:** TLS-Zertifikate und Caddy-Konfiguration.

### 8.3 Environment-Konfiguration

Sensible Konfigurationswerte werden über eine `.env`-Datei injiziert:

- `DATABASE_URL` – MariaDB-Verbindungsstring
- `JWT_SECRET` – Signaturschlüssel für JWT-Tokens
- `OAUTH_GOOGLE_CLIENT_ID` / `SECRET`
- `OAUTH_GITHUB_CLIENT_ID` / `SECRET`
- `DOMAIN` – Öffentliche Domain für Caddy (Let's Encrypt)
- `RUST_LOG` – Log-Level für das Backend

### 8.4 Backup-Strategie

- **Datenbank:** Täglicher `mysqldump` per Cronjob, komprimiert, Rotation der letzten 14 Tage.
- **Media-Dateien:** Inkrementelles Backup via `rsync` auf externen Speicher.
- **Konfiguration:** `docker-compose.yml` und `.env` werden im Git-Repository versioniert (ohne Secrets).

---

## 9. Projektstruktur

Das Monorepo ist für folgende Zielstruktur geplant (noch nicht im Repository angelegt):

```
lilly/
├── frontend/                 # SvelteKit PWA
│   ├── src/
│   │   ├── routes/           # SvelteKit File-Based Routing
│   │   ├── lib/
│   │   │   ├── components/   # Wiederverwendbare UI-Komponenten
│   │   │   ├── stores/       # Svelte Stores (Sammlung, Auth)
│   │   │   ├── api/          # API-Client (fetch-Wrapper)
│   │   │   └── i18n/         # Paraglide.js Übersetzungen
│   │   └── service-worker.ts
│   ├── static/               # Statische Assets, PWA-Manifest
│   └── Dockerfile
├── backend/                  # Rust / Axum API
│   ├── src/
│   │   ├── main.rs
│   │   ├── routes/           # API-Endpunkte (auth, collection, trades)
│   │   ├── models/           # Datenstrukturen / DTOs
│   │   ├── db/               # SQLx-Queries, Migrations
│   │   ├── auth/             # JWT, OAuth, argon2
│   │   └── services/         # Business-Logik (Matching etc.)
│   ├── migrations/           # SQLx-Datenbankmigrationen
│   └── Dockerfile
├── importer/                 # Wiki-Datenimport CLI
│   ├── src/
│   │   ├── main.rs
│   │   ├── adapters/         # WikiAdapter-Trait + Implementierungen
│   │   │   ├── mod.rs
│   │   │   ├── maddraxikon.rs
│   │   │   └── gruselroman.rs
│   │   └── db.rs             # Shared DB-Zugriff
│   └── Dockerfile
├── docker-compose.yml
├── Caddyfile
├── .env.example
├── LICENSE
└── README.md
```

---

## 10. Sicherheitsarchitektur

- **TLS:** Caddy erzwingt HTTPS für alle Verbindungen. HTTP wird automatisch auf HTTPS umgeleitet.
- **CORS:** Strikte CORS-Policy – nur die eigene Domain ist als Origin erlaubt.
- **Rate Limiting:** Tower-Middleware im Axum-Backend: 10 Requests/Minute für Auth-Endpunkte, 100 Requests/Minute für allgemeine API-Nutzung.
- **Input-Validierung:** Alle Eingaben werden serverseitig validiert (serde + validator-Crate). SQL Injection wird durch SQLx-Prepared-Statements verhindert.
- **XSS:** SvelteKit escaped Output automatisch. User-generierte Inhalte (Notizen, Kommentare) werden zusätzlich serverseitig sanitized.
- **CSRF:** API-Calls sind durch JWT im Authorization-Header geschützt (kein Cookie). Der Refresh-Token wird jedoch als httpOnly-Cookie übertragen, daher ist der Endpunkt `/api/v1/auth/refresh` prinzipiell CSRF-anfällig. Schutzmaßnahmen: `SameSite=Strict` auf dem Refresh-Cookie, serverseitige Validierung des `Origin`-Headers, und Beschränkung des Refresh-Endpunkts auf das Ausstellen neuer Tokens (keine zustandsändernde Geschäftslogik).
- **Upload-Sicherheit:** Nur JPEG, PNG und WebP erlaubt. Maximale Dateigröße: 5 MB. Dateien werden serverseitig re-encoded (image-Crate), um Exploits in Bild-Metadaten zu eliminieren.
- **Datenschutz:** E-Mail-Adressen werden verschlüsselt gespeichert (AES-256-GCM). Account-Löschung entfernt alle personenbezogenen Daten inklusive Fotos.

---

*Ende des Architektur- und Designdokuments*