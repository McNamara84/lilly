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

| Komponente            | Technologie                | Details                                                                                   |
| --------------------- | -------------------------- | ----------------------------------------------------------------------------------------- |
| **Frontend**          | Svelte 5 / SvelteKit       | Runes-Reaktivität, SSR + CSR, Manifest und SvelteKit-Service-Worker                       |
| **UI-Framework**      | Skeleton UI + Tailwind CSS | Svelte-native Komponentenbibliothek, Tailwind für Utility-First-Styling, Dark/Light Mode  |
| **Backend / API**     | Rust + Axum                | Async HTTP-Framework auf Basis von Tokio, Tower-Middleware, modularer Router              |
| **Datenbank**         | MariaDB 12.3 LTS           | Relationale Datenbank, InnoDB-Engine, Volltextsuche, bewährte MySQL-Kompatibilität        |
| **DB-Zugriff**        | SQLx                       | Compile-time verified SQL-Queries, async, kein ORM-Overhead, Migrations-System            |
| **Authentifizierung** | Eigenbau: JWT + argon2id   | Access/Refresh-Token-Paar, argon2id für Passwort-Hashing, OAuth2-Client für Google/GitHub |
| **API-Spezifikation** | OpenAPI 3.1 / Swagger      | Generiert via utoipa-Crate (Rust), Swagger-UI als Dev-Tool                                |
| **Dateispeicher**     | Lokales Dateisystem        | Strukturiertes Verzeichnis, Caddy Static Serving, automatische Bildkompression            |
| **Reverse Proxy**     | Caddy v2                   | Automatisches HTTPS via Let's Encrypt, minimale Konfiguration, HTTP/2 + HTTP/3            |
| **Containerisierung** | Docker + Docker Compose    | Multi-Container-Setup, isolierte Services, einfaches Deployment                           |
| **Wiki-Importer**     | Rust (reqwest)             | CLI-Tool und Cronjob-fähig, strukturierte MediaWiki-API, modulare Adapter                 |
| **i18n**              | Paraglide.js (SvelteKit)   | Typsichere Übersetzungen, Tree-Shaking, initiale Sprache Deutsch                          |

### 2.2 Begründung der Kernentscheidungen

**Svelte 5 / SvelteKit als Frontend**

Svelte 5 kompiliert Komponenten zur Build-Zeit zu optimiertem JavaScript, wodurch kein Framework-Runtime-Overhead im Browser entsteht. Das neue Runes-System bietet fein-granulare Reaktivität. SvelteKit liefert Routing, SSR, die `$service-worker`-Buildlisten und die Build-Pipeline aus einer Hand. LILLY verwendet den eigenen SvelteKit-Service-Worker ohne zusätzliches PWA-Plugin. Skeleton UI bietet als Svelte-native Komponentenbibliothek hochwertige, barrierefreie UI-Komponenten auf Tailwind-Basis.

**Rust mit Axum als Backend**

Rust bietet Memory Safety ohne Garbage Collector und ermöglicht extrem ressourceneffiziente Server-Anwendungen – ideal für Self-Hosting auf einem einzelnen VPS. Axum ist das modernste async Web-Framework im Rust-Ökosystem, aufgebaut auf dem bewährten Tokio-Runtime und dem Tower-Middleware-Stack. SQLx als Datenbankschicht prüft SQL-Queries bereits zur Compile-Zeit gegen das tatsächliche Datenbankschema, was eine ganze Klasse von Laufzeitfehlern eliminiert.

**MariaDB als Datenbank**

MariaDB ist ein ausgereiftes, performantes RDBMS mit vollständiger MySQL-Kompatibilität. Die relationalen Datenstrukturen von LILLY (Serien, Hefte, Sammlungen, Tausche) profitieren von referentieller Integrität und JOIN-Operationen. MariaDB bietet zudem integrierte Volltextsuche, die für die Heft- und Seriensuche genutzt werden kann. Die breite Hosting-Kompatibilität erleichtert Self-Hosting und Community-Deployments.

**Caddy als Reverse Proxy**

Caddy v2 bietet automatisches HTTPS über integriertes ACME-Protokoll (Let's Encrypt) mit minimalem Konfigurationsaufwand. Caddy unterstützt HTTP/2 und HTTP/3 out-of-the-box und stellt ausschließlich öffentliche Referenzcover statisch bereit. Private Sammlungsfotos werden dagegen immer zugriffskontrolliert über die Backend-API ausgeliefert.

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
│                  │   MediaWiki → MariaDB   │                  │
│                  └────────────────────────┘                   │
└──────────────────────────────────────────────────────────────┘
```

### 3.2 Container-Übersicht (Docker Compose)

Das System besteht aus fünf Docker-Containern, orchestriert via Docker Compose:

| Container  | Image                                       | Port (intern)    | Aufgabe                                                                           |
| ---------- | ------------------------------------------- | ---------------- | --------------------------------------------------------------------------------- |
| `caddy`    | `caddy:2.11.4-alpine`                       | 80, 443 → extern | HTTPS-Terminierung, Reverse Proxy, statische Referenzcover unter `/media/covers/` |
| `frontend` | `node:26.7.0-alpine` + Build                | 3000 (intern)    | SvelteKit SSR-Server, liefert PWA-Shell und pre-rendered Pages                    |
| `backend`  | `rust:1.97.1-trixie` + `debian:trixie-slim` | 8080 (intern)    | REST API (Axum), Authentifizierung, Business-Logik, Bildverarbeitung              |
| `db`       | `mariadb:12.3.2`                            | 3306 (intern)    | Persistente Datenhaltung, Volltextindex                                           |
| `importer` | Rust CLI (eigener Build)                    | –                | Cronjob-basierter Wiki-Datenimport, schreibt direkt in MariaDB                    |

### 3.3 Request-Flow

Der typische Ablauf einer Nutzeranfrage:

1. **Client → Caddy:** Alle eingehenden Requests landen bei Caddy (Port 443). Caddy terminiert TLS und routet basierend auf dem Pfad.
2. **Caddy → Frontend:** Seiten-Requests (HTML, JS, CSS) werden an den SvelteKit-Server (Port 3000) weitergeleitet. SvelteKit liefert SSR-gerenderte Seiten oder die PWA-Shell.
3. **Caddy → Backend:** API-Requests unter `/api/*` werden direkt an den Axum-Server (Port 8080) geroutet.
4. **Caddy → Dateisystem:** Nur Referenzcover unter `/media/covers/*` werden statisch aus dem gemounteten Volume ausgeliefert. Nutzerfotos laufen immer über den autorisierenden API-Endpunkt; andere `/media/*`-Pfade liefern `404`.
5. **Backend → MariaDB:** Der Axum-Server kommuniziert über SQLx mit MariaDB für alle Datenoperationen.

---

## 4. Datenbankschema

Das folgende Schema definiert die Kernentitäten und ihre Beziehungen. Alle Tabellen verwenden InnoDB als Storage Engine und UTF-8mb4 als Zeichensatz.

### 4.1 Tabelle: `series`

| Spalte             | Typ          | Constraint              | Beschreibung                                                                                                                                   |
| ------------------ | ------------ | ----------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------- |
| `id`               | INT UNSIGNED | PK, AUTO_INC            | Primärschlüssel                                                                                                                                |
| `name`             | VARCHAR(255) | NOT NULL, UQ            | Serienname (z. B. „Maddrax – Die dunkle Zukunft der Erde")                                                                                     |
| `slug`             | VARCHAR(255) | NOT NULL, UQ            | URL-freundlicher Bezeichner (z. B. „maddrax")                                                                                                  |
| `publisher`        | VARCHAR(255) | NULL                    | Verlag                                                                                                                                         |
| `genre`            | VARCHAR(100) | NULL                    | Genre (Science-Fiction, Horror, Western etc.)                                                                                                  |
| `frequency`        | VARCHAR(50)  | NULL                    | Erscheinungsrhythmus (wöchentlich, 14-tägig etc.)                                                                                              |
| `total_issues`     | INT UNSIGNED | NULL                    | Aktuelle Gesamtzahl Hefte (NULL bei laufenden Serien)                                                                                          |
| `status`           | ENUM         | NOT NULL                | 'running' \| 'completed' \| 'cancelled'                                                                                                        |
| `active`           | BOOLEAN      | NOT NULL, DEF 0         | Ob die Serie für normale Nutzer sichtbar ist. Importierte Serien starten als inaktiv und müssen von einem Admin nach Prüfung aktiviert werden. |
| `source_key`       | VARCHAR(64)  | NULL, UQ mit Quell-ID   | Stabile Quellenart, z. B. `maddraxikon`                                                                                                        |
| `source_record_id` | VARCHAR(255) | NULL, UQ mit Quellenart | Stabile ID des Serienrecords in der Quelle                                                                                                     |
| `source_url`       | VARCHAR(500) | NULL                    | URL der Datenquelle (Wiki)                                                                                                                     |
| `created_at`       | TIMESTAMP    | NOT NULL                | Erstellungszeitpunkt                                                                                                                           |
| `updated_at`       | TIMESTAMP    | NOT NULL                | Letzter Sync-Zeitpunkt                                                                                                                         |

### 4.2 Tabelle: `issues`

| Spalte             | Typ          | Constraint                                 | Beschreibung                                                  |
| ------------------ | ------------ | ------------------------------------------ | ------------------------------------------------------------- |
| `id`               | INT UNSIGNED | PK, AUTO_INC                               | Primärschlüssel                                               |
| `series_id`        | INT UNSIGNED | FK, NOT NULL                               | Fremdschlüssel auf series.id                                  |
| `issue_number`     | INT UNSIGNED | NOT NULL                                   | Heftnummer innerhalb der Serie                                |
| `title`            | VARCHAR(500) | NOT NULL                                   | Titel des Heftes                                              |
| `author`           | VARCHAR(500) | NULL                                       | Autor(en), kommasepariert                                     |
| `published_at`     | DATE         | NULL                                       | Ersterscheinungsdatum                                         |
| `cycle`            | VARCHAR(255) | NULL                                       | Zyklus / Handlungsabschnitt                                   |
| `cover_url`        | VARCHAR(500) | NULL                                       | URL zum Cover-Bild in der Wiki-Quelle                         |
| `cover_local_path` | VARCHAR(500) | NULL                                       | Relativer Pfad zum lokal gespeicherten Cover im /media-Volume |
| `source_key`       | VARCHAR(64)  | NULL, UQ mit Quell-ID, paarweise gesetzt   | Stabile Quellenart                                            |
| `source_record_id` | VARCHAR(255) | NULL, UQ mit Quellenart, paarweise gesetzt | Stabile ID des Hefts in der Quelle                            |
| `source_wiki_url`  | VARCHAR(500) | NULL                                       | Link zum Wiki-Eintrag des Heftes                              |
| `created_at`       | TIMESTAMP    | NOT NULL                                   | Import-Zeitpunkt                                              |

_Unique Indizes: `(series_id, issue_number)` verhindert doppelte Hefteinträge pro Serie;
`(source_key, source_record_id)` verhindert doppelte Quellenidentitäten. Eine Check-Constraint
erlaubt für die beiden Provenienzfelder nur gemeinsam `NULL` oder gemeinsam gesetzte Werte._

### 4.3 Tabelle: `users`

| Spalte              | Typ          | Constraint           | Beschreibung                                                                            |
| ------------------- | ------------ | -------------------- | --------------------------------------------------------------------------------------- |
| `id`                | INT UNSIGNED | PK, AUTO_INC         | Primärschlüssel                                                                         |
| `email`             | VARCHAR(255) | NOT NULL, UQ         | E-Mail-Adresse (verschlüsselt gespeichert)                                              |
| `password_hash`     | VARCHAR(255) | NULL                 | argon2id-Hash (NULL bei reinem OAuth-Login)                                             |
| `display_name`      | VARCHAR(100) | NOT NULL             | Anzeigename / Sammlername                                                               |
| `role`              | ENUM         | NOT NULL, DEF 'user' | 'user' \| 'admin' — Benutzerrolle. Admins können Imports starten und Serien verwalten.  |
| `avatar_path`       | VARCHAR(500) | NULL                 | Interner zufälliger Storage Key des normalisierten Avatars; wird nie per API ausgegeben |
| `location`          | VARCHAR(255) | NULL                 | Standort (freiwillig, für Tausch-Nähe)                                                  |
| `profile_public`    | BOOLEAN      | NOT NULL, DEF 0      | Profil öffentlich sichtbar?                                                             |
| `collection_public` | BOOLEAN      | NOT NULL, DEF 0      | Sammlung einschließlich persönlicher Heftnotizen öffentlich sichtbar?                   |
| `created_at`        | TIMESTAMP    | NOT NULL             | Registrierungszeitpunkt                                                                 |

OAuth-Identitäten und Einwilligungen sind normalisiert:

- `oauth_identities`: `user_id`, Provider, unveränderliches Provider-Subject sowie letzter Login;
  Unique-Constraints auf `(provider, provider_subject)` und `(user_id, provider)` verhindern
  Identitätsübernahme und doppelte Verknüpfungen.
- `privacy_consents`: unveränderliche Policy-Version, Zeitpunkt und Registrierungsweg
  (`password`, `google`, `github`, `legacy`). Die Fremdschlüssel werden bei Kontolöschung
  kaskadiert.
- `oauth_authorization_flows` und `pending_oauth_links`: ausschließlich kurzlebige, serverseitige
  Zustände. State, Browserbindung und Link-Token werden nur gehasht gespeichert und nach Ablauf
  bereinigt.

#### 4.3.1 Tabelle: `password_reset_tokens`

| Spalte       | Typ             | Constraint            | Beschreibung                                        |
| ------------ | --------------- | --------------------- | --------------------------------------------------- |
| `id`         | BIGINT UNSIGNED | PK, AUTO_INC          | Technischer Schlüssel                               |
| `user_id`    | INT UNSIGNED    | FK, NOT NULL, CASCADE | Zugehöriges Passwortkonto                           |
| `token_hash` | CHAR(64)        | NOT NULL, UNIQUE      | SHA-256-Hash; der rohe Token wird nicht persistiert |
| `created_at` | DATETIME(6)     | NOT NULL              | Ausstellungszeitpunkt                               |
| `expires_at` | DATETIME(6)     | NOT NULL, INDEX       | Ablaufzeitpunkt                                     |
| `used_at`    | DATETIME(6)     | NULL                  | Verbrauch beziehungsweise Invalidierung             |

Das Ersetzen und Verbrauchen erfolgt transaktional. Beim erfolgreichen Verbrauch werden alle noch
aktiven Reset-Tokens des Nutzers markiert, sein Passwort aktualisiert und sämtliche Refresh-Tokens
widerrufen.

### 4.4 Tabelle: `collection_entries`

| Spalte            | Typ              | Constraint      | Beschreibung                                                                                                                            |
| ----------------- | ---------------- | --------------- | --------------------------------------------------------------------------------------------------------------------------------------- |
| `id`              | INT UNSIGNED     | PK, AUTO_INC    | Primärschlüssel                                                                                                                         |
| `user_id`         | INT UNSIGNED     | FK, NOT NULL    | Fremdschlüssel auf users.id (ON DELETE CASCADE)                                                                                         |
| `issue_id`        | INT UNSIGNED     | FK, NOT NULL    | Fremdschlüssel auf issues.id                                                                                                            |
| `copy_number`     | TINYINT UNSIGNED | NOT NULL, DEF 1 | Technische Exemplarnummer; bei automatischer Vergabe die kleinste freie Nummer von 1 bis 255                                            |
| `edition_label`   | VARCHAR(120)     | NULL            | Optionale, getrimmte Editionsbezeichnung; leerer Text wird als `NULL` gespeichert                                                       |
| `condition_grade` | ENUM             | NULL            | 'Z0' \| 'Z1' \| 'Z2' \| 'Z3' \| 'Z4'; nur bei `wanted` optional, bei `owned` und `duplicate` durch die Domänenvalidierung verpflichtend |
| `status`          | ENUM             | NOT NULL        | 'owned' \| 'duplicate' \| 'wanted'                                                                                                      |
| `notes`           | TEXT             | NULL            | Persönliche Notizen                                                                                                                     |
| `created_at`      | TIMESTAMP        | NOT NULL        | Zeitpunkt der Erfassung                                                                                                                 |
| `updated_at`      | TIMESTAMP        | NOT NULL        | Letzte Änderung                                                                                                                         |

_Unique Index: `(user_id, issue_id, copy_number)` – ein Nutzer kann dasselbe Heft mehrfach erfassen, aber jedes physische Exemplar ist unabhängig von seiner optionalen Editionsbezeichnung eindeutig identifiziert. Der zusätzliche Index `(status, issue_id, user_id)` beschleunigt Angebots-, Wunsch- und Matching-Abfragen._

### 4.5 Tabelle: `collection_photos`

| Spalte        | Typ          | Constraint       | Beschreibung                                                           |
| ------------- | ------------ | ---------------- | ---------------------------------------------------------------------- |
| `id`          | INT UNSIGNED | PK, AUTO_INC     | Primärschlüssel                                                        |
| `entry_id`    | INT UNSIGNED | FK, NOT NULL     | Fremdschlüssel auf collection_entries.id (ON DELETE CASCADE)           |
| `storage_key` | VARCHAR(128) | UNIQUE, NOT NULL | Servergenerierter, nicht erratbarer Schlüssel im privaten Media-Volume |
| `media_type`  | VARCHAR(32)  | NOT NULL         | Kanonischer MIME-Typ des gespeicherten Derivats                        |
| `byte_size`   | INT UNSIGNED | NOT NULL         | Größe des normalisierten Derivats                                      |
| `width`       | INT UNSIGNED | NOT NULL         | Verifizierte Breite des Derivats                                       |
| `height`      | INT UNSIGNED | NOT NULL         | Verifizierte Höhe des Derivats                                         |
| `sort_order`  | TINYINT      | NOT NULL, 0–3    | Stabiler Foto-Slot innerhalb des Sammlungsexemplars                    |
| `created_at`  | TIMESTAMP    | NOT NULL         | Upload-Zeitpunkt                                                       |

_Unique Index: `(entry_id, sort_order)` – zusammen mit einer `FOR UPDATE`-Sperre des
Sammlungseintrags verhindert er, dass parallele Uploads das Viererlimit überschreiten. Offene
Dateilöschungen werden in `media_deletion_jobs` persistiert und beim Backend-Start idempotent
wiederholt._

### 4.6 Tabelle: `import_jobs`

| Spalte                                | Typ          | Constraint              | Beschreibung                                                                                                 |
| ------------------------------------- | ------------ | ----------------------- | ------------------------------------------------------------------------------------------------------------ |
| `id`                                  | INT UNSIGNED | PK, AUTO_INC            | Primärschlüssel                                                                                              |
| `series_id`                           | INT UNSIGNED | FK, NOT NULL            | Fremdschlüssel auf series.id (ON DELETE CASCADE)                                                             |
| `adapter_name`                        | VARCHAR(100) | NOT NULL                | Name des verwendeten Import-Adapters (z. B. „maddrax")                                                       |
| `source_key`                          | VARCHAR(64)  | NULL                    | Snapshot der verwendeten Quellenart                                                                          |
| `status`                              | ENUM         | NOT NULL, DEF 'pending' | `pending` \| `running` \| `completed` \| `completed_with_errors` \| `failed` \| `cancelled` \| `interrupted` |
| `total_issues`                        | INT UNSIGNED | NOT NULL, DEF 0         | Anzahl der im Vollscan gemeldeten Quellhefte                                                                 |
| `imported_issues`                     | INT UNSIGNED | NOT NULL, DEF 0         | Legacy-Aggregat aus `created + updated + unchanged`                                                          |
| `created_issues` / `updated_issues`   | INT UNSIGNED | NOT NULL, DEF 0         | Neu angelegte und fachlich geänderte Hefte                                                                   |
| `unchanged_issues` / `skipped_issues` | INT UNSIGNED | NOT NULL, DEF 0         | Unveränderte und bewusst übersprungene Hefte                                                                 |
| `failed_issues`                       | INT UNSIGNED | NOT NULL, DEF 0         | Recordbezogen fehlgeschlagene Hefte                                                                          |
| `error_message`                       | TEXT         | NULL                    | Kompakte Fehlerzusammenfassung                                                                               |
| `cancel_requested_at`                 | DATETIME     | NULL                    | Persistenter kooperativer Abbruchwunsch                                                                      |
| `cancel_requested_by`                 | INT UNSIGNED | FK, NULL                | Erster Admin, der den Abbruch angefordert hat; `ON DELETE SET NULL`                                          |
| `retry_of_job_id`                     | INT UNSIGNED | FK, NULL                | Verknüpfung zum Ursprungslauf                                                                                |
| `started_by`                          | INT UNSIGNED | FK, NULL                | Admin bei manuellen Läufen; NULL beim Scheduler                                                              |
| `started_at` / `completed_at`         | DATETIME     | NULL                    | Laufzeitgrenzen                                                                                              |
| `created_at` / `updated_at`           | DATETIME     | NOT NULL                | Anlage und letzter persistierter Fortschritt                                                                 |

Recordbezogene Fehler liegen zusätzlich in `import_job_errors` mit Job, Quelle, optionaler Heftnummer und Quell-ID, Verarbeitungsstufe und Meldung. Verwaiste aktive Jobs werden nach einem Neustart als `interrupted` sichtbar; sie werden nicht still fortgesetzt.

Rollenbeförderungen werden separat in `role_change_events` gespeichert. Das Audit enthält
vorherige und neue Rolle, Methode (`admin_email_bootstrap` oder `cli`) und Zeitpunkt. Es
entsteht nur bei einer echten Rollenänderung und bleibt nach einer Accountlöschung erhalten.

### 4.7 Tabellen: trades, messages, comments

Die verbleibenden Tabellen folgen demselben Muster. Hier eine kompakte Übersicht der Kernfelder:

**trades**

- `id`, `initiator_id` (FK users), `responder_id` (FK users), `status` ENUM('proposed', 'accepted', 'completed', 'cancelled'), `completed_at`, `created_at`, `updated_at`

**trade_items**

- Unveränderliche Snapshots der vereinbarten Ausgabe, Zustandsnote und angebotenen/gewünschten Edition; die Referenz auf den abgegebenen Sammlungseintrag darf beim Abschluss durch `ON DELETE SET NULL` entfallen.

**trade_completion_confirmations**

- `trade_id`, `user_id`, `confirmed_at`; der zusammengesetzte Primärschlüssel erlaubt genau eine Abschlussbestätigung je Teilnehmer.

**messages**

- `id`, `sender_id` (FK users), `recipient_id` (FK users), `trade_id` (FK trades, NULL), `content` TEXT, `is_read` BOOLEAN, `created_at`

**comments**

- `id`, `user_id` (FK users), `issue_id` (FK issues), `text` TEXT, `rating` TINYINT (1–5), `created_at`, `updated_at`

---

## 5. API-Design

Alle Endpunkte sind unter dem Präfix `/api/v1` erreichbar. Authentifizierte Endpunkte erfordern einen gültigen JWT im `HttpOnly`-Access-Cookie.

### 5.1 Endpunkt-Übersicht

| Methode             | Pfad                                              | Auth        | Beschreibung                                                                            |
| ------------------- | ------------------------------------------------- | ----------- | --------------------------------------------------------------------------------------- |
| **POST**            | `/api/v1/auth/register`                           | Nein        | Registrierung (E-Mail/Passwort)                                                         |
| **POST**            | `/api/v1/auth/login`                              | Nein        | Login → Access + Refresh Token                                                          |
| **GET**             | `/api/v1/auth/options`                            | Nein        | Provider-Verfügbarkeit und aktuelle Datenschutzversion                                  |
| **POST**            | `/api/v1/auth/oauth/{provider}/start`             | Nein        | OAuth-Login/-Registrierung mit State, Browserbindung und PKCE starten                   |
| **GET**             | `/api/v1/auth/oauth/{provider}/callback`          | Nein        | Provider-Callback prüfen und Login/Registrierung abschließen                            |
| **GET/POST/DELETE** | `/api/v1/auth/oauth/link`                         | Optional/Ja | Ausstehende explizite Kontoverknüpfung lesen, bestätigen oder abbrechen                 |
| **POST**            | `/api/v1/auth/refresh`                            | Refresh     | Token-Erneuerung                                                                        |
| **GET**             | `/api/v1/auth/me`                                 | Ja          | Aktueller Nutzer (inkl. Rolle)                                                          |
| **POST**            | `/api/v1/auth/logout`                             | Ja          | Logout (Cookies löschen)                                                                |
| **GET**             | `/api/v1/auth/verify`                             | Nein        | E-Mail-Verifizierung per Token                                                          |
| **POST**            | `/api/v1/auth/resend-verification`                | Nein        | Verifizierungs-E-Mail erneut senden                                                     |
| **POST**            | `/api/v1/auth/password-reset/request`             | Nein        | Generische Reset-Anfrage ohne Offenlegung des Kontostatus                               |
| **POST**            | `/api/v1/auth/password-reset/confirm`             | Nein        | Einmaligen Reset-Token verbrauchen und neues Passwort setzen                            |
| **GET**             | `/api/v1/me/privacy-consents`                     | Ja          | Private, versionierte Einwilligungshistorie des eigenen Kontos                          |
| **GET**             | `/api/v1/series`                                  | Nein        | Alle **aktiven** Serien auflisten                                                       |
| **GET**             | `/api/v1/series/{slug}/issues`                    | Nein        | Alle Hefte einer aktiven Serie (paginiert)                                              |
| **GET**             | `/api/v1/issues/{id}`                             | Nein        | Heft-Details + Community-Kommentare                                                     |
| **GET**             | `/api/v1/me/collection`                           | Ja          | Eigene Sammlung (Filter, Paginierung)                                                   |
| **POST**            | `/api/v1/me/collection`                           | Ja          | Heft zur Sammlung hinzufügen                                                            |
| **PATCH**           | `/api/v1/me/collection/{id}`                      | Ja          | Eintrag ändern (Zustand, Status, Edition, Notizen)                                      |
| **DELETE**          | `/api/v1/me/collection/{id}`                      | Ja          | Eintrag entfernen                                                                       |
| **POST**            | `/api/v1/me/collection/{id}/photos`               | Ja          | Foto hochladen (multipart/form-data)                                                    |
| **GET**             | `/api/v1/me/collection/{id}/photos`               | Ja          | Eigene Fotos des exakten Sammlungsexemplars auflisten                                   |
| **DELETE**          | `/api/v1/me/collection/{id}/photos/{photo_id}`    | Ja          | Eigenes Foto einzeln löschen                                                            |
| **GET**             | `/api/v1/collection-photos/{photo_id}/content`    | Optional    | Foto für Eigentümer oder bei öffentlicher Sammlung ausliefern                           |
| **GET**             | `/api/v1/media/photo-policy`                      | Nein        | Nicht-sensitive Uploadgrenzen und unterstützte Bildtypen                                |
| **GET**             | `/api/v1/me/trade-offers`                         | Ja          | Eigene aktive Tauschangebote aus Einträgen mit Status `duplicate` (Filter, Paginierung) |
| **GET**             | `/api/v1/me/wanted`                               | Ja          | Eigene aktive Wunschliste (Filter, Paginierung)                                         |
| **GET**             | `/api/v1/me/wanted/candidates`                    | Ja          | Nicht vorhandene Hefte einer aktiven Serie samt Wunschstatus                            |
| **POST**            | `/api/v1/me/wanted/bulk`                          | Ja          | Bis zu 100 Hefte idempotent zur Wunschliste hinzufügen                                  |
| **DELETE**          | `/api/v1/me/wanted/{entry_id}`                    | Ja          | Eigenen Wunschlisteneintrag entfernen                                                   |
| **GET**             | `/api/v1/me/matches`                              | Ja          | Aktive wechselseitige Matches (paginiert)                                               |
| **GET**             | `/api/v1/me/matches/{match_id}`                   | Ja          | Eigenes Match mit datenschutzsicherer Partner- und Itemprojektion                       |
| **POST**            | `/api/v1/me/matches/{match_id}/proposals`         | Ja          | Aus ausgewählten Match-Items einen Tausch und Thread erzeugen                           |
| **GET**             | `/api/v1/me/trades`                               | Ja          | Eigene offene oder geschlossene Tausche (`scope=open/closed`)                           |
| **GET**             | `/api/v1/me/trades/{trade_id}`                    | Ja          | Tausch-Snapshot einschließlich Thread-ID                                                |
| **POST**            | `/api/v1/me/trades/{trade_id}/accept`             | Ja          | Vorschlag als Empfänger annehmen                                                        |
| **POST**            | `/api/v1/me/trades/{trade_id}/complete`           | Ja          | Erhalt bestätigen; zweite Bestätigung überträgt die Sammlung atomar                     |
| **POST**            | `/api/v1/me/trades/{trade_id}/cancel`             | Ja          | Offenen Tausch als Teilnehmer abbrechen                                                 |
| **GET**             | `/api/v1/me/messages`                             | Ja          | Thread-Inbox mit Vorschau und Ungelesen-Zähler                                          |
| **GET/POST**        | `/api/v1/me/messages/{thread_id}`                 | Ja          | Nachrichten lesen oder idempotent senden                                                |
| **PATCH**           | `/api/v1/me/messages/{thread_id}/read`            | Ja          | Empfangene Nachrichten bis zu einer ID gelesen markieren                                |
| **GET**             | `/api/v1/me/notifications`                        | Ja          | Eigene Benachrichtigungen, optional nur ungelesene                                      |
| **GET**             | `/api/v1/me/notifications/unread-count`           | Ja          | Anzahl ungelesener Benachrichtigungen                                                   |
| **PATCH**           | `/api/v1/me/notifications/{notification_id}/read` | Ja          | Einzelne Benachrichtigung gelesen markieren                                             |
| **POST**            | `/api/v1/me/notifications/read-all`               | Ja          | Alle eigenen Benachrichtigungen gelesen markieren                                       |
| **GET**             | `/api/v1/me/collection/stats`                     | Ja          | Sammlungsstatistiken (Gesamt, pro Serie, Doppelte, Gesuchte)                            |
| **GET**             | `/api/v1/me/activity`                             | Ja          | Letzte Aktivitäten des Nutzers (Timeline)                                               |
| **GET**             | `/api/v1/me/profile`                              | Ja          | Eigenes Profil + Sichtbarkeitseinstellungen                                             |
| **PATCH**           | `/api/v1/me/profile`                              | Ja          | Eigenen Anzeigenamen und optionalen Standort ändern                                     |
| **POST**            | `/api/v1/me/profile/avatar`                       | Ja          | Eigenen Avatar validieren, normalisieren und hochladen/ersetzen                         |
| **DELETE**          | `/api/v1/me/profile/avatar`                       | Ja          | Eigenen Avatar samt wiederholbarer Dateibereinigung entfernen                           |
| **PATCH**           | `/api/v1/me/profile/visibility`                   | Ja          | Profil- und Sammlungssichtbarkeit ändern                                                |
| **GET**             | `/api/v1/users/{user_id}/profile`                 | Nein        | Öffentliches Profil (wenn freigegeben)                                                  |
| **GET**             | `/api/v1/users/{user_id}/avatar`                  | Optional    | Avatar für Eigentümer oder bei öffentlichem Profil ausliefern                           |
| **GET**             | `/api/v1/users/{user_id}/collection`              | Nein        | Öffentliche Sammlung einschließlich Notizen (wenn freigegeben)                          |
| **GET**             | `/api/v1/users/{user_id}/collection/stats`        | Nein        | Profilstatistik, wenn Profil und Sammlung öffentlich sind                               |
| **GET**             | `/api/v1/users`                                   | Nein        | Öffentliche Sammler-Liste (sortier-/filterbar)                                          |
| **GET**             | `/api/v1/issues/most-wanted`                      | Nein        | Meistgesuchte Hefte plattformweit                                                       |

#### 5.1.1 Admin-Endpunkte

Alle Admin-Endpunkte erfordern einen authentifizierten Nutzer mit der Rolle `admin`. Bei fehlendem Admin-Recht wird HTTP 403 (Forbidden) zurückgegeben.

| Methode  | Pfad                                       | Auth  | Beschreibung                                                                                    |
| -------- | ------------------------------------------ | ----- | ----------------------------------------------------------------------------------------------- |
| **GET**  | `/api/v1/admin/series`                     | Admin | Alle Serien (inkl. inaktive) auflisten                                                          |
| **POST** | `/api/v1/admin/series/{slug}/activate`     | Admin | Kompatibilitätsroute; verweigert eine ungeprüfte Aktivierung mit `review_required`              |
| **POST** | `/api/v1/admin/series/{slug}/deactivate`   | Admin | Serie ausblenden und append-only Auditereignis schreiben                                        |
| **GET**  | `/api/v1/admin/adapters`                   | Admin | Verfügbare Import-Adapter einschließlich Quellenart auflisten                                   |
| **POST** | `/api/v1/admin/import`                     | Admin | Vollscan anlegen (`{ "adapter": "maddrax" }`) → HTTP 202 mit Job                                |
| **GET**  | `/api/v1/admin/import/{id}`                | Admin | Import-Job-Status & Fortschritt abfragen                                                        |
| **POST** | `/api/v1/admin/import/{id}/cancel`         | Admin | Persistenten Abbruch für einen aktiven Job anfordern                                            |
| **POST** | `/api/v1/admin/import/{id}/retry`          | Admin | Verknüpften neuen Vollscan für einen fehlgeschlagenen oder abgebrochenen Job anlegen            |
| **GET**  | `/api/v1/admin/import/{id}/errors`         | Admin | Persistierte Fehlerkontexte paginiert lesen                                                     |
| **GET**  | `/api/v1/admin/import/{id}/review/summary` | Admin | Laufbezogene Ergebnis-, Risiko-, Referenz- und Freigabeauswertung lesen                         |
| **GET**  | `/api/v1/admin/import/{id}/review/items`   | Admin | Persistierte Ergebnisse des konkreten Laufs paginiert suchen und filtern                        |
| **POST** | `/api/v1/admin/import/{id}/activate`       | Admin | Geprüften, aktuellen und vollständigen Lauf atomar freigeben; Warnungen müssen quittiert werden |
| **GET**  | `/api/v1/admin/import/{id}/series-issues`  | Admin | Aktuellen Serienbestand paginiert lesen (Legacy-/Diagnoseendpunkt, nicht freigaberelevant)      |
| **GET**  | `/api/v1/admin/import/history`             | Admin | Import-Historie aller Jobs                                                                      |

Veröffentlichungsereignisse bleiben als append-only Historie erhalten: Serien mit vorhandenen Auditereignissen können nicht gelöscht werden. Wird ein Benutzerkonto gelöscht, wird lediglich der Akteursbezug des Ereignisses entfernt; die historische Freigabeentscheidung selbst bleibt bestehen.

### 5.2 Abgeleiteter Status "Fehlend" (missing)

Der Status `missing` wird **nicht** in der Datenbank gespeichert. Er ist ein abgeleiteter (virtueller) Status, der sich aus der Differenz zwischen der Gesamtmenge der Hefte einer Serie (`issues`-Tabelle) und den Sammlungseinträgen des Nutzers (`collection_entries`-Tabelle) ergibt.

**Berechnung:** Ein Heft gilt als `missing`, wenn für die Kombination `(user_id, issue_id)` kein Eintrag in `collection_entries` existiert.

**API-Verhalten bei `GET /api/v1/me/collection?status=missing`:**

Wird der Filter `status=missing` angefragt, führt das Backend einen LEFT JOIN von `issues` (gefiltert nach Serie) auf `collection_entries` (gefiltert nach User) durch und liefert nur die Hefte zurück, die **keinen** zugehörigen Sammlungseintrag haben. Die Response enthält dann Issue-Objekte ohne `collection_entry`-Daten.

**API-Verhalten bei `GET /api/v1/series/{slug}/issues` (authentifiziert):**

Wenn ein authentifizierter Nutzer die Heftliste einer Serie abruft, reichert das Backend die Response optional mit dem Sammlungsstatus pro Heft an (owned/duplicate/wanted/null). Hefte mit `null`-Status gelten im Frontend als `missing`.

#### 5.2.1 Zählregeln für Sammlungsstatistiken

Die Statistik trennt physische Exemplare von logischen Ausgaben:

- `total_physical_owned` zählt jeden Sammlungseintrag mit Status `owned` oder `duplicate`.
  Mehrere physische Exemplare derselben Ausgabe werden mehrfach gezählt; `wanted` zählt nicht.
- `total_owned` und `series_stats[].owned_count` zählen unterschiedliche `issue_id`-Werte mit
  Status `owned` oder `duplicate`. Dadurch erhöhen Doppelexemplare oder alternative Editionen den
  Serienfortschritt nicht mehrfach.
- Ein Prozentwert wird nur bei einer bekannten positiven Gesamtzahl ausgegeben und auf höchstens
  100 Prozent begrenzt. Bei unbekannter Gesamtzahl bleiben Nenner und Prozentwert `null`.
- Die eigene Statistik ist immer authentifiziert abrufbar. Die Profilstatistik ist öffentlich nur
  verfügbar, wenn `profile_public` und `collection_public` gesetzt sind. Der direkte öffentliche
  Sammlungslink folgt weiterhin ausschließlich `collection_public`.

Avatar-Uploads verwenden dieselbe inhaltsbasierte JPEG-/PNG-/WebP-Prüfung und Normalisierung wie
Sammlungsfotos. API-Antworten enthalten nur die kontrollierte URL
`/api/v1/users/{user_id}/avatar`; der Storage Key bleibt intern. Beim Ersetzen oder Löschen wird
der alte Key über `media_deletion_jobs` wiederholbar bereinigt. Die Storage-Reconciliation
berücksichtigt sowohl Sammlungsfotos als auch aktive Avatare.

### 5.3 Abgeleitete Tausch- und Wunschlisten

Tauschangebote und Wünsche verwenden `collection_entries` als einzige Datenquelle. Ein Eintrag mit Status `duplicate` ist automatisch ein aktives Angebot; `wanted` ist automatisch ein aktiver Wunsch. Statuswechsel und Löschungen benötigen daher weder Synchronisationsjobs noch zusätzliche Listentabellen.

Neue Wünsche werden ohne Zustandsbewertung gespeichert, weil noch kein physisches Exemplar vorliegt. Beim Wechsel eines solchen Eintrags auf `owned` oder `duplicate` muss die Anfrage einen gültigen Zustand Z0–Z4 enthalten. Ein vorhandener Zustand sowie Notizen und Fotos bleiben bei Statuswechseln erhalten. Ein Wunsch ohne Edition ist ein Platzhalter für jede Edition; mit `edition_label` wird nur eine gleich bezeichnete angebotene Edition berücksichtigt.

Die Kandidatenabfrage verlangt `series_slug`, berücksichtigt nur aktive Serien und schließt eigene `owned`- und `duplicate`-Einträge aus. Bereits gesuchte Hefte bleiben mit `is_wanted = true` sichtbar. Die Bulk-Anlage dedupliziert höchstens 100 positive Heft-IDs, sperrt Hefte in stabiler Reihenfolge und meldet pro ID `created`, `unchanged` oder `rejected`. Alle neuen Listenendpunkte sind privat; öffentliche Freigaben sind eine separate Ausbaustufe.

Ein angenommener Tausch wird erst nach Bestätigung beider Teilnehmer abgeschlossen. Die zweite Bestätigung validiert die reservierten Sammlungseinträge gegen die unveränderlichen Item-Snapshots und überträgt beide Richtungen innerhalb derselben Transaktion. Empfangene Exemplare werden `owned`; persönliche Absendernotizen und -fotos wechseln nicht den Besitzer. Abgeschlossene und abgebrochene Tausche sind über `scope=closed` weiterhin mit ihrem Nachrichten-Thread abrufbar.

### 5.4 Authentifizierung

Die Authentifizierung basiert auf einem JWT-Paar:

- **Access Token:** Kurzlebig (15 Minuten), wird als httpOnly-Cookie gespeichert. Enthält user_id, display_name und role als Claims.
- **Refresh Token:** Langlebig (30 Tage), wird als httpOnly-Cookie gespeichert. Dient ausschließlich zur Erneuerung des Access Tokens.
- **Rollenwechsel:** Bereits ausgestellte Access Tokens behalten ihre Rolle bis zum Ablauf. Ein Refresh liest die aktuelle Rolle aus MariaDB und stellt danach einen Token mit der neuen Rolle aus.
- **OAuth2 Flow:** Authorization Code Flow mit PKCE S256, zufälligem State und zusätzlicher
  HttpOnly-Browserbindung für Google und GitHub. Vorgänge sind zehn Minuten gültig und einmalig.
  Nur verifizierte Provider-E-Mails und stabile Subjects werden akzeptiert; Zugriffstoken bleiben
  flüchtig. Ein E-Mail-Treffer verknüpft niemals automatisch, sondern erfordert Login am passenden
  Bestandskonto und einen ausdrücklichen Bestätigungs-POST.
- **Datenschutz-Einwilligung:** Passwort- und OAuth-Registrierung speichern Policy-Version,
  Zeitpunkt und Registrierungsweg atomar mit dem Konto. Eine zwischen Anzeige und Abschluss
  geänderte Version wird mit `PRIVACY_POLICY_CHANGED` zurückgewiesen.
- **Passwort-Hashing:** argon2id mit empfohlenen Parametern (m=19456, t=2, p=1).
- **Passwort-Wiederherstellung:** Ein 256-Bit-Token wird ausschließlich als SHA-256-Hash
  gespeichert und ist standardmäßig 60 Minuten gültig. Nur das jüngste Token eines verifizierten
  Passwortkontos ist aktiv. Ein erfolgreicher Reset setzt das Passwort und widerruft alle
  Refresh-Sessions in derselben Transaktion; vorhandene Access-Tokens enden spätestens nach ihrer
  kurzen TTL von standardmäßig 15 Minuten.

---

## 6. Datenimport-Architektur

### 6.1 Modulares Adapter-System

Die generische Import-Logik ist als eigenständiges Rust-Crate (`importer-core`)
implementiert. Quellenspezifische Parser, Fixtures und HTTP-Zugriffe liegen getrennt in
`importer-adapters`. Das Backend komponiert beide Crates und startet Imports über die
Admin-WebUI als asynchrone Tokio-Background-Tasks.

Das Kernkonzept ist ein Adapter-Pattern mit Trait-basierter Architektur:

- **Trait `WikiAdapter`:** Definiert `source_descriptor()`, `fetch_series_metadata()`, `fetch_issue_list()`, `fetch_issue_details(number)` und `fetch_cover(number, known_source_sha1)`. Der statische Descriptor identifiziert Quelle und Zielserie bereits vor dem ersten Netzwerkzugriff.
- **`AdapterRegistry`:** Zentrale Registrierung aller verfügbaren Adapter. Doppelte Namen werden abgelehnt, die Ausgabe ist deterministisch sortiert. Das Backend erhält die produktive Registry ausschließlich von `importer-adapters::builtin_registry()` und stellt sie via `AppState` bereit.
- **`ProgressReporter`-Trait:** Entkoppelt die Fortschrittsmeldung von der Persistenz. Das Backend implementiert dieses Trait mit DB-Writes in die `import_jobs`-Tabelle, das CLI könnte es mit stdout-Output implementieren.
- **`MaddraxAdapter` (v1.0):** Adapter für de.maddraxikon.com. Nutzt strukturierte MediaWiki-API-Antworten für Metadaten und die exakte, heftnummernbezogene Coverauswahl.

**Crate-Struktur:**

```
importer-core/             # Quellenunabhängiger Vertrag
└── src/
    ├── lib.rs             # Öffentliche API und Re-Exports
    ├── adapter.rs         # WikiAdapter-Trait + AdapterRegistry
    ├── contract.rs        # Wiederverwendbare Adapter-Vertragsprüfung
    ├── types.rs           # SeriesData, IssueData, CoverData, CoverIdentity
    └── progress.rs        # ProgressReporter-Trait
importer-adapters/         # Konkrete Quellen und lokale Fixtures
├── src/
│   ├── lib.rs             # builtin_registry()
│   └── adapters/
│       ├── maddrax.rs
│       └── john_sinclair.rs
└── tests/fixtures/
```

### 6.2 Import-Ablauf

1. **Jobanlage:** Der Adminstart löst die Zielserie nur über den statischen Adapter-Descriptor auf, persistiert einen `pending`-Job und antwortet mit HTTP 202. Im Request findet kein Wiki-Zugriff statt.
2. **Asynchrone Ausführung:** Ein `tokio::spawn`-Task markiert den Job konditional als `running`; MariaDB bleibt die alleinige Quelle für Status und Fortschritt.
3. **Quellenprüfung:** Serien- und Heftdaten werden normalisiert und gegen `source_key`, Quell-ID, HTTPS-Host und Pflichtfelder validiert.
4. **Vollscan:** Jeder Lauf liest die aktuelle Heftliste vollständig. Jedes gemeldete Heft erhält genau ein Ergebnis: `created`, `updated`, `unchanged`, `skipped` oder `failed`.
5. **Idempotenter Vergleich:** Vorhandene Metadaten und Relationen werden gebündelt geladen. Nur neue oder geänderte Datensätze werden atomar geschrieben; unveränderte Relationen bleiben unberührt.
6. **Cover:** Die technische MediaWiki-Dateiidentität wird bei jedem Vollscan verglichen. Nur fehlende oder geänderte Bildrevisionen werden geladen; ein Coverfehler zerstört keine validen bibliografischen Daten.
7. **Recovery:** Abbruchwünsche werden vor Abruf und Persistenz geprüft. Neustart-Waisen enden als `interrupted`; ein zulässiger Retry erzeugt einen neuen verknüpften Vollscan.
8. **Polling und Prüfung:** Die Adminseite pollt den Job alle drei Sekunden, zeigt Detailzähler und Fehlerkontext und stoppt bei jedem terminalen Status.
9. **Aktivierung:** Importierte Serien bleiben inaktiv, bis ein Admin die Stichprobe geprüft und die Serie explizit aktiviert hat.

Die verbindlichen Hosts, Quell-IDs, Feldmappings und Referenz-Fixtures sind in [`docs/import-sources.md`](import-sources.md) dokumentiert.

### 6.3 Hinzufügen neuer Serien

Um eine neue Serie (z. B. Perry Rhodan via Perrypedia) zu integrieren, sind folgende Schritte erforderlich:

- Neuen Adapter unter `importer-adapters/src/adapters/` implementieren und mit lokalen Fixtures gegen `verify_adapter_contract` testen.
- Adapter einmalig in `importer-adapters::builtin_registry()` registrieren.
- Backend neu bauen und deployen — der neue Adapter erscheint automatisch in der Admin-UI.
- Admin startet den Import über die WebUI und aktiviert die Serie nach Prüfung.

Es sind keine Änderungen am Frontend, an der Datenbank oder an der Backend-Kern-Logik notwendig.
Eine vollständige Anleitung einschließlich Minimalbeispiel und Testmatrix steht in
[Neuen Import-Adapter hinzufügen](adding-import-adapter.md).

---

## 7. PWA und Offline-Strategie

### 7.1 Service Worker

SvelteKit baut `frontend/src/service-worker.ts` zusammen mit dem versionierten Web-App-Manifest. Der Service Worker implementiert folgende Caching-Strategien:

- **App Shell:** Statische Manifest-/Icon-Assets werden vorab gespeichert. Besuchte versionierte JavaScript-/CSS-Buildartefakte werden Cache First abgelegt; Navigationsantworten für `/`, `/login`, `/collection` und `/collection/add` verwenden Network First mit Shell-Fallback.
- **API-Daten:** `/api/**` wird niemals im Service-Worker-Cache gespeichert. Sammlungs- und Katalogdaten werden online bevorzugt geladen und benutzergebunden in IndexedDB geschrieben.
- **Bilder:** Öffentliche Referenzcover können „Stale While Revalidate“ verwenden. Persönliche Fotos werden mit `private, no-store` ausgeliefert und nicht in einen gemeinsamen Service-Worker-Cache aufgenommen.
- **Updates:** Alte `lilly-*`-Caches werden erst bei Aktivierung entfernt. Eine wartende Version wird angezeigt und erst nach ausdrücklicher Nutzeraktion aktiviert; die Mutationsqueue ist zu diesem Zeitpunkt bereits persistent.

### 7.2 Offline-Fähigkeit

- **Lesen:** Ein authentifizierter Snapshot enthält ausschließlich den aktuellen Benutzerbezug, die aktiven MVP-Kataloge und die eigene Sammlung. `profiles`, `series`, `issues`, `collection_entries`, `mutations`, `conflicts` und `metadata` sind getrennte IndexedDB-Stores; private Schlüssel enthalten immer die Benutzer-ID.
- **Offline-Auth:** Nur ein echter Netzwerkfehler darf den zuletzt online bestätigten Benutzerkontext wiederherstellen. Eine 401-/Refresh-Ablehnung öffnet niemals eine Offline-Sitzung.
- **Schreiben:** Anlegen und Ändern werden zuerst mit Client-UUID und Basisrevision dauerhaft gespeichert und anschließend optimistisch dargestellt. Sync startet beim App-Start, beim `online`-Ereignis und manuell. Solange die App offline ist, prüft sie zusätzlich alle fünf Sekunden den nicht cachebaren Health-Endpunkt und synchronisiert bei wiederhergestellter Erreichbarkeit; das fängt Browser ab, die nach einem Service-Worker-Reload kein zuverlässiges `online`-Ereignis liefern.
- **Idempotenz und Konflikte:** Das Backend speichert `(user_id, mutation_id)` samt Request-Fingerprint und Ergebnis. Wiederholungen ändern nichts doppelt; veraltete Revisionen werden geparkt. Der Nutzer entscheidet sichtbar zwischen Serverstand und erneutem Anwenden des lokalen Stands.
- **Logout:** Profilkontext, Sammlung, Queue, Konflikte und Snapshot-Metadaten des abgemeldeten Benutzers werden gelöscht. Der unpersönliche App-Shell-Cache darf bestehen bleiben.
- **Fotos:** Foto-Uploads sind im MVP bewusst onlinepflichtig. Ausgewählte Dateien werden nicht dauerhaft in einer Offline-Queue abgelegt; die UI meldet Übertragungsfehler und lässt vorhandene Fotos unverändert.
- **Tausch:** Tausch-Funktionen erfordern eine aktive Internetverbindung.

### 7.3 Performance-Gates

Lighthouse CI misst `/privacy` und die authentifizierte Route `/collection` jeweils fünfmal mit einem festgelegten mobilen Profil. Der Median muss einen First Contentful Paint von höchstens 3.000 ms und einen Performance-Score von mindestens 0,90 erreichen. Die Authentifizierung nutzt das deterministische E2E-Konto; JSON-/HTML-Berichte bleiben als private GitHub-Actions-Artefakte erhalten.

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
- `PASSWORD_RESET_TTL_SECONDS` – Gültigkeit eines Passwort-Reset-Links (Standard: 3600 Sekunden)
- `TRUSTED_PROXY_CIDRS` – einzige Netze, deren Forwarding-Header das Backend auswertet
- `RATE_LIMIT_*` – Policy-Grenzen im Format `MAX_REQUESTS/WINDOW_SECONDS`
- `ADMIN_EMAIL` – normalisierte E-Mail-Adresse eines bestehenden Kontos; beim Serverstart wird eine echte Beförderung transaktional auditiert
- `MEDIA_PATH` – Pfad zum Media-Volume für Cover-Bilder und Nutzer-Uploads (Standard: `/media`)
- `OAUTH_GOOGLE_CLIENT_ID` / `SECRET`
- `OAUTH_GITHUB_CLIENT_ID` / `SECRET`
- `DOMAIN` – Öffentliche Domain für Caddy (Let's Encrypt)
- `RUST_LOG` – Log-Level für das Backend

Weitere Admins werden ohne Passwortübergabe über denselben transaktionalen Rollenservice
befördert: `lilly-backend admin promote --email user@example.org`. Eine Beförderung liefert
Exitcode 0, eine bereits bestehende Adminrolle 4, ungültige Eingaben 2, unbekannte Konten 3
und Datenbankfehler 1.

### 8.4 Backup-Strategie

- **Datenbank:** Täglicher `mysqldump` per Cronjob, komprimiert, Rotation der letzten 14 Tage.
- **Media-Dateien:** Inkrementelles Backup via `rsync` auf externen Speicher.
- **Konfiguration:** `docker-compose.yml` und `.env.example` werden im Git-Repository versioniert. Die eigentliche `.env`-Datei enthält Secrets und wird über `.gitignore` ausgeschlossen; sie wird ausschließlich lokal oder über einen Secret-Manager bereitgestellt.

---

## 9. Projektstruktur

Das Monorepo ist für folgende Zielstruktur geplant (noch nicht im Repository angelegt):

```
lilly/
├── Cargo.toml                # Workspace-Root (backend, importer, importer-core, importer-adapters)
├── frontend/                 # SvelteKit PWA
│   ├── src/
│   │   ├── routes/           # SvelteKit File-Based Routing
│   │   │   ├── admin/        # Admin-Bereich (eigener Routenpräfix)
│   │   │   │   ├── series/   # Serien-Verwaltung
│   │   │   │   └── import/   # Import starten & Prüfansicht
│   │   │   └── series/       # Öffentliche Serien-Ansicht
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
│   │   ├── routes/           # API-Endpunkte (auth, series, admin, health)
│   │   ├── models/           # Datenstrukturen / DTOs
│   │   ├── db/               # SQLx-Queries (users, series, issues, import_jobs)
│   │   ├── auth/             # JWT, OAuth, argon2, AdminUser-Guard
│   │   └── services/         # Business-Logik (Email, Import-Orchestrierung)
│   ├── migrations/           # SQLx-Datenbankmigrationen
│   └── Dockerfile
├── importer-core/            # Quellenunabhängiger WikiAdapter-Vertrag
│   └── src/
│       ├── lib.rs            # Öffentliche API
│       ├── adapter.rs        # WikiAdapter-Trait + AdapterRegistry
│       ├── types.rs          # SeriesData, IssueData, CoverData
│       ├── progress.rs       # ProgressReporter-Trait
│       └── contract.rs       # Gemeinsame Adapter-Vertragsprüfung
├── importer-adapters/        # Built-in-Adapter, Parser und Offline-Fixtures
│   ├── src/adapters/         # Maddrax und John Sinclair
│   └── tests/fixtures/       # Deterministische Quellantworten
├── importer/                 # Wiki-Datenimport CLI (nutzt importer-core)
│   ├── src/
│   │   └── main.rs           # CLI-Wrapper
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
- **Rate Limiting:** Ein zentraler Sliding-Window-Limiter schützt die gesamte API. Öffentliche
  Aufrufe sind standardmäßig auf 120/Minute je Client begrenzt, authentifizierte Aufrufe auf
  600/Minute je Client und Nutzer. Login, Registrierung, OAuth, Refresh, Verifikationsmail und
  Passwort-Reset besitzen engere, separat konfigurierbare Policies. `429` liefert sowohl
  `Retry-After` als auch `retry_after_seconds`. Der In-Memory-Speicher ist für eine einzelne
  Backend-Instanz vorgesehen; mehrere Instanzen benötigen später einen gemeinsamen Store.
- **Proxy-Vertrauen:** `Forwarded`, `X-Forwarded-For` und `X-Real-IP` werden nur akzeptiert, wenn
  der unmittelbare Socket-Peer in `TRUSTED_PROXY_CIDRS` liegt. Die Referenzkonfiguration
  überschreibt die Kette am öffentlichen Nginx-Eingang und wertet sie danach von rechts aus.
- **Input-Validierung:** Alle Eingaben werden serverseitig validiert (serde + validator-Crate). SQL Injection wird durch SQLx-Prepared-Statements verhindert.
- **XSS:** SvelteKit escaped Output automatisch. User-generierte Notizen werden ausschließlich als Text gespeichert und gerendert; ungeprüftes HTML wird nicht ausgegeben.
- **CSRF:** Access- und Refresh-Token liegen in `HttpOnly`-Cookies mit `SameSite=Lax`; mutierende
  Browseraufrufe verwenden JSON und die CORS-Policy erlaubt nur die eigene Origin. OAuth nutzt
  zusätzlich einmaligen State, PKCE und eine gebundene Browserkennung.
- **Passwort-Reset:** Unbekannte, unbestätigte, reine OAuth- und berechtigte Adressen erhalten
  denselben Status und Body. Rohe Reset-Tokens werden weder gespeichert noch protokolliert,
  parallele Bestätigungen werden per Zeilensperre auf genau einen Erfolg begrenzt und alle
  Refresh-Tokens werden atomar widerrufen.
- **Upload-Sicherheit:** Nur erfolgreich dekodierbare JPEG-, PNG- und WebP-Inhalte sind erlaubt;
  Dateiname und Client-MIME-Typ werden nicht vertraut. Maximale Eingabegröße: 5 MiB. Container,
  Abmessungen und Pixelzahl werden vor teurer Verarbeitung begrenzt. Das Backend korrigiert die
  Orientierung, skaliert ohne Hochskalierung auf maximal 2048 px, entfernt Metadaten und erzeugt
  ein kanonisches JPEG-Derivat. Caddy veröffentlicht ausschließlich Referenzcover unter
  `/media/covers/*`; Nutzerfotos werden nach Owner-/Privacy-Prüfung durch die API gestreamt.
- **Datenschutz:** E-Mail-Adressen werden normalisiert und nur für notwendige Kontofunktionen
  verwendet. Rate-Limit-Schlüssel für Account, Token, Client und Nutzer sind geheime
  SHA-256-Fingerprints. Beim Löschen eines Accounts entfernt die Datenbankkaskade die
  Fotozuordnungen; der idempotente Storage-Abgleich beseitigt die danach verwaisten Dateien.

---

_Ende des Architektur- und Designdokuments_
