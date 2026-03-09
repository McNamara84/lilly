# LILLY – Implementierungsplan

**Version 1.0** | Stand: 08. März 2026

---

## Getroffene Entscheidungen

| Thema | Entscheidung |
|---|---|
| Package Manager | npm |
| Startseite | Nur Login-Seite (fertig & funktional via Docker testbar) |
| Backend-Strategie | Vollständiges Rust/Axum-Backend mit MariaDB, Migrationen & Seed-Daten |
| E2E-Test-Framework | Playwright |
| Linting/Formatting | ESLint + Prettier (Frontend), Clippy + rustfmt (Backend) |
| UI-Framework | Skeleton (`@skeletonlabs/skeleton-svelte` v4.x) mit Tailwind CSS v4 |
| Node.js | v22 LTS (gemäß architecture.md) |
| Rust Edition | 2021 |

---

## Phasenübersicht

| Phase | Beschreibung | Abhängigkeiten |
|---|---|---|
| 1 | Monorepo-Struktur & Docker-Infrastruktur | – |
| 2 | Frontend-Projekt einrichten | Phase 1 |
| 3 | Backend-Projekt einrichten | Phase 1 |
| 4 | Datenbank: Migrationen & Seed-Daten | Phase 3 |
| 5 | Login-Seite implementieren (Frontend + Backend-Auth) | Phase 2, 4 |
| 6 | Testing-Frameworks einrichten & Tests schreiben | Phase 5 |
| 7 | Statische Code-Analyse einrichten | Phase 2, 3 |
| 8 | GitHub CI/CD Workflows | Phase 6, 7 |
| 9 | README aktualisieren | Phase 8 |

---

## Phase 1: Monorepo-Struktur & Docker-Infrastruktur

### 1.1 Verzeichnisstruktur anlegen

```
lilly/
├── frontend/                 # SvelteKit PWA (Phase 2)
├── backend/                  # Rust / Axum API (Phase 3)
├── importer/                 # Wiki-Importer CLI (nur Platzhalter)
├── docs/                     # Planungsdokumente (existiert)
├── docker-compose.yml        # Kompletter Stack
├── Caddyfile                 # Reverse Proxy Config
├── .env.example              # Environment Template
├── .gitignore                # Aktualisiert für Rust + Node
└── README.md                 # Aktualisiert (Phase 9)
```

### 1.2 Docker Compose (`docker-compose.yml`)

Vier Container für den initialen Stack:

| Service | Image | Port | Beschreibung |
|---|---|---|---|
| `caddy` | `caddy:2-alpine` | 80 → Host | Reverse Proxy, routet `/api/*` → Backend, Rest → Frontend |
| `frontend` | `node:22-alpine` + Build | 3000 intern | SvelteKit Dev/Preview Server |
| `backend` | Rust Multi-Stage Build | 8080 intern | Axum REST API |
| `db` | `mariadb:11` | 3306 intern | MariaDB mit persisten Volume |

- `depends_on` Ketten: frontend/backend → db, caddy → frontend + backend
- Health Checks auf allen Services
- Volume `db_data` für MariaDB-Persistenz
- Volume `caddy_data` für Zertifikate

### 1.3 Caddyfile

```
:80 {
    handle /api/* {
        reverse_proxy backend:8080
    }
    handle {
        reverse_proxy frontend:3000
    }
}
```

Lokale Entwicklung auf HTTP Port 80. HTTPS-Konfiguration (Let's Encrypt) kommt bei Produktionsdeployment.

### 1.4 `.env.example`

```env
# Database
DATABASE_URL=mysql://lilly:lilly_secret@db:3306/lilly
MARIADB_ROOT_PASSWORD=root_secret
MARIADB_DATABASE=lilly
MARIADB_USER=lilly
MARIADB_PASSWORD=lilly_secret

# Auth
JWT_SECRET=change-me-in-production
JWT_ACCESS_TOKEN_EXPIRY=900
JWT_REFRESH_TOKEN_EXPIRY=2592000

# Backend
RUST_LOG=info
BACKEND_PORT=8080

# Frontend
PUBLIC_API_BASE_URL=http://localhost/api/v1
```

### 1.5 `.gitignore` aktualisieren

Ergänzungen für Rust (`target/`, `*.lock` für importer), Node (`node_modules/`, `.svelte-kit/`, `build/`), Environment (`.env`), Docker und IDE-Dateien.

---

## Phase 2: Frontend-Projekt einrichten

### 2.1 SvelteKit-Projekt erstellen

- `npm create svelte@latest frontend` mit TypeScript, ESLint, Prettier, Playwright
- Svelte 5 mit Runes-Syntax

### 2.2 Dependencies installieren

**Produktions-Dependencies:**

| Paket | Zweck |
|---|---|
| `@skeletonlabs/skeleton-svelte` | Skeleton UI-Komponenten (v4.x) |
| `@tailwindcss/vite` | Tailwind CSS v4 Vite-Integration |
| `tailwindcss` (v4) | Utility-First CSS |
| `lucide-svelte` | Icon-Bibliothek |
| `@fontsource/inter` | Inter Font (Self-Hosted) |

**Dev-Dependencies:**

| Paket | Zweck |
|---|---|
| `vitest` | Unit/Integration Test Runner |
| `@testing-library/svelte` | Svelte Component Testing |
| `@testing-library/jest-dom` | DOM Assertions |
| `@playwright/test` | E2E Tests |
| `eslint` | Linter |
| `eslint-plugin-svelte` | Svelte-spezifische ESLint-Regeln |
| `prettier` | Code-Formatter |
| `prettier-plugin-svelte` | Svelte Prettier Support |
| `@sveltejs/adapter-node` | Node-Adapter für Docker Deployment |

### 2.3 Tailwind CSS v4 konfigurieren

Tailwind v4 nutzt CSS-first Configuration. In `frontend/src/app.css`:

```css
@import "tailwindcss";
@import "@fontsource/inter";

@theme {
  --color-brand-primary-50: #ecfeff;
  --color-brand-primary-500: #06b6d4;
  --color-brand-primary-900: #164e63;
  /* ... alle Design Tokens aus design-tokens.json */

  --color-condition-z0: #10b981;
  --color-condition-z5: #ef4444;
  /* ... */

  --font-sans: 'Inter', ui-sans-serif, system-ui, sans-serif;
}

/* Glassmorphism Utility Classes */
/* Dark/Light Mode CSS Custom Properties */
```

Die Design Tokens aus `docs/design-tokens.json` werden vollständig in die `@theme`-Direktive überführt.

### 2.4 Skeleton konfigurieren

- Skeleton Theme in `+layout.svelte` einbinden
- Custom Theme erstellen, das zu den LILLY Design Tokens passt (Cyan-Primary)

### 2.5 Verzeichnisstruktur

```
frontend/src/
├── routes/
│   ├── +layout.svelte          # App Shell (minimal für Login)
│   ├── +page.svelte            # Redirect zu /login (wenn nicht auth)
│   └── login/
│       └── +page.svelte        # Login-Seite
├── lib/
│   ├── components/
│   │   └── layout/             # TopBar (minimal)
│   ├── api/
│   │   └── auth.ts             # Auth API Client
│   └── i18n/                   # Platzhalter für Paraglide.js
├── app.css                     # Tailwind + Design Tokens
└── app.html                    # HTML Template
```

### 2.6 Dockerfile (Frontend)

Multi-Stage Build:
1. **Build Stage:** `node:22-alpine` → `npm ci` → `npm run build`
2. **Run Stage:** `node:22-alpine` → nur `build/` + `node_modules/` → `node build`

Adapter: `@sveltejs/adapter-node`

---

## Phase 3: Backend-Projekt einrichten

### 3.1 Rust-Projekt erstellen

```bash
cargo init backend
```

### 3.2 Dependencies (`Cargo.toml`)

| Crate | Zweck |
|---|---|
| `axum` | HTTP Framework |
| `tokio` (full) | Async Runtime |
| `sqlx` (mysql, runtime-tokio, tls-rustls) | Datenbankzugriff |
| `serde` + `serde_json` | Serialisierung |
| `jsonwebtoken` | JWT-Token-Handling |
| `argon2` | Passwort-Hashing |
| `tower-http` (cors, trace) | HTTP Middleware |
| `tracing` + `tracing-subscriber` | Structured Logging |
| `validator` + `derive` | Input-Validierung |
| `dotenvy` | .env-Dateien laden |
| `utoipa` + `utoipa-swagger-ui` | OpenAPI 3.1 Dokumentation |
| `thiserror` | Error-Handling |

### 3.3 Projektstruktur

```
backend/src/
├── main.rs              # Server Startup, Router, Middleware
├── config.rs            # Environment-Konfiguration
├── routes/
│   ├── mod.rs
│   ├── auth.rs          # POST /api/v1/auth/login
│   └── health.rs        # GET /api/v1/health
├── models/
│   ├── mod.rs
│   └── user.rs          # User-Struct, Login-DTOs
├── db/
│   ├── mod.rs
│   └── users.rs         # SQLx-Queries für User-Tabelle
├── auth/
│   ├── mod.rs
│   ├── jwt.rs           # JWT-Erzeugung & -Validierung
│   └── password.rs      # argon2id Hashing & Verify
├── error.rs             # AppError-Typ (IntoResponse)
└── services/            # Platzhalter
```

### 3.4 Implementierte Endpunkte

| Methode | Pfad | Beschreibung |
|---|---|---|
| `GET` | `/api/v1/health` | Health Check (DB-Konnektivität) |
| `POST` | `/api/v1/auth/login` | Login mit E-Mail + Passwort → JWT |

Der Login-Endpunkt:
- Validiert Input (E-Mail-Format, Passwort nicht leer)
- Sucht User in MariaDB per E-Mail
- Verifiziert Passwort mit argon2id
- Gibt Access Token (JWT, 15 min) zurück
- Refresh Token (httpOnly Cookie, 30 Tage) und Rate Limiting (10 Requests/Minute) sind für einen späteren PR geplant

### 3.5 Error Handling

Einheitlicher `AppError`-Typ:
- `BadRequest(String)` → 400
- `Unauthorized(String)` → 401
- `NotFound(String)` → 404
- `InternalError(String)` → 500

Implementiert `IntoResponse` mit JSON-Body: `{ "error": "message" }`.

### 3.6 Dockerfile (Backend)

Multi-Stage Build:
1. **Build Stage:** `rust:1-slim` → `cargo build --release`
2. **Run Stage:** `debian:bookworm-slim` → nur Binary + TLS-Zertifikate

SQLx wird mit `SQLX_OFFLINE=true` gebaut (offline-Modus mit `sqlx-data.json`).

---

## Phase 4: Datenbank – Migrationen & Seed-Daten

### 4.1 Migration: `users`-Tabelle

SQLx-Migration (`backend/migrations/`):

```sql
CREATE TABLE users (
    id INT UNSIGNED NOT NULL AUTO_INCREMENT PRIMARY KEY,
    email VARCHAR(255) NOT NULL UNIQUE,
    password_hash VARCHAR(255) NULL,
    display_name VARCHAR(100) NOT NULL,
    avatar_path VARCHAR(500) NULL,
    location VARCHAR(255) NULL,
    profile_public BOOLEAN NOT NULL DEFAULT FALSE,
    oauth_provider VARCHAR(50) NULL,
    oauth_id VARCHAR(255) NULL,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;
```

### 4.2 Seed-Daten

Ein Demo-User wird per Migration eingefügt:

| Feld | Wert |
|---|---|
| E-Mail | `demo@lilly.app` |
| Passwort | `demo1234` (argon2id-gehashed) |
| Display Name | `Demo-Sammler` |

Dieser User ermöglicht sofortiges Testen der Login-Seite via Docker.

### 4.3 Migrationen ausführen

Migrationen werden automatisch beim Start des Backend-Containers ausgeführt (`sqlx::migrate!().run(&pool).await`).

---

## Phase 5: Login-Seite implementieren

### 5.1 Frontend: Login-Seite (`/login`)

Gemäß `screens.json` → `auth_login`:

- Zentrierte Glassmorphism-Karte (max 420px) auf Gradient-Hintergrund
- LILLY Logo + Tagline
- E-Mail Input-Feld
- Passwort Input-Feld
- Login Button (Brand Primary)
- Divider: „oder weiter mit"
- OAuth-Buttons (Google, GitHub) — visuell vorhanden, nicht funktional (disabled mit Tooltip)
- Links: „Passwort vergessen?" und „Noch kein Konto? Registrieren" — als Text vorhanden, aber nicht verlinkt (Seiten existieren noch nicht)

Technische Details:
- Form-Handling mit Svelte 5 Runes (`$state` für Formularfelder)
- Client-seitige Validierung (E-Mail-Format, Passwort nicht leer)
- API-Call an `POST /api/v1/auth/login`
- Access Token im Memory speichern
- Fehlerbehandlung (falsche Credentials → Fehlermeldung anzeigen)
- Loading State während API-Call (Button disabled + Spinner)

### 5.2 Frontend: Minimale App Shell

- `+layout.svelte` mit Dark/Light Mode Toggle
- Minimal-TopBar mit LILLY Logo
- Keine Sidebar/BottomNav (noch keine navigierbaren Seiten)

### 5.3 Frontend: API Client

`frontend/src/lib/api/auth.ts`:
- `login(email: string, password: string): Promise<LoginResponse>`
- Typisierte Request/Response-Interfaces
- Zentraler `fetch`-Wrapper mit Error Handling

### 5.4 Glassmorphism-Styling

CSS Custom Properties für Dark/Light Mode gemäß `design-tokens.json`:
- `.glass-card` — Standard-Container
- `.glass-elevated` — Für Login-Karte
- Animierter Gradient-Mesh-Hintergrund (subtil, CSS-only)

### 5.5 Inter Font

Self-Hosted via `@fontsource/inter`, gebündelt im Frontend Build.

---

## Phase 6: Testing-Frameworks einrichten & Tests schreiben

### 6.1 Frontend: Unit Tests (Vitest + Testing Library)

**Framework-Setup:**
- `vitest.config.ts` mit Svelte-Plugin und jsdom-Environment
- `@testing-library/svelte` für Component Rendering
- `@testing-library/jest-dom` für DOM-Matchers
- Coverage-Reporter: `v8` mit Threshold 70%

**Tests für Login-Seite:**

| Test | Beschreibung |
|---|---|
| `login-page.test.ts` | Rendert Login-Formular, zeigt alle Elemente (E-Mail, Passwort, Button, OAuth-Buttons) |
| | Validierung: leere Felder → Fehlermeldung |
| | Validierung: ungültiges E-Mail-Format → Fehlermeldung |
| | Erfolgreicher Login → API wird aufgerufen |
| | Fehlgeschlagener Login → Fehlernachricht wird angezeigt |
| | Loading State während API-Call |
| `auth-api.test.ts` | API Client: erfolgreicher Login gibt Token zurück |
| | API Client: 401 Response wird korrekt behandelt |
| | API Client: Netzwerkfehler wird behandelt |

### 6.2 Frontend: Integration/Regression Tests (Vitest)

Dieselbe Vitest-Infrastruktur, aber Tests, die mehrere Komponenten zusammen testen:

| Test | Beschreibung |
|---|---|
| `login-integration.test.ts` | Gesamter Login-Flow: Formular ausfüllen → abschicken → Response verarbeiten |
| | Dark/Light Mode Toggle verändert Theme korrekt |

### 6.3 Frontend: E2E Tests (Playwright)

**Framework-Setup:**
- `playwright.config.ts` mit Chromium, Firefox, WebKit
- Base URL: `http://localhost:80` (Docker)
- Screenshot on Failure
- HTML Reporter

**Tests:**

| Test | Beschreibung |
|---|---|
| `login.spec.ts` | Login-Seite lädt korrekt |
| | Login mit Demo-Credentials (`demo@lilly.app` / `demo1234`) funktioniert |
| | Login mit falschen Credentials zeigt Fehlermeldung |
| | Responsive: Login-Karte ist auf Mobile und Desktop korrekt dargestellt |
| | Accessibility: Fokus-Reihenfolge ist korrekt, alle Felder haben Labels |

### 6.4 Backend: Unit Tests (Rust `#[cfg(test)]`)

| Modul | Tests |
|---|---|
| `auth/password.rs` | Hash-Erstellung, Verifikation, falsches Passwort schlägt fehl |
| `auth/jwt.rs` | Token-Erstellung, Token-Validierung, abgelaufener Token wird abgelehnt |
| `error.rs` | AppError → korrekte HTTP-Status-Codes und JSON-Body |
| `routes/auth.rs` | Login-Validierung: leere E-Mail, leeres Passwort |

### 6.5 Backend: Integration Tests (Rust `#[tokio::test]`)

| Test | Beschreibung |
|---|---|
| `tests/auth_integration.rs` | Login mit gültigen Credentials → 200 + Token |
| | Login mit falschen Credentials → 401 |
| | Login mit ungültigem Input → 400 |
| | Health-Check Endpunkt → 200 |

Diese Tests nutzen eine Test-Datenbank (separater MariaDB-Container oder In-Memory-Testdaten).

### 6.6 npm Scripts

```json
{
  "test": "vitest run",
  "test:watch": "vitest",
  "test:coverage": "vitest run --coverage",
  "test:e2e": "playwright test",
  "test:e2e:ui": "playwright test --ui"
}
```

---

## Phase 7: Statische Code-Analyse

### 7.1 Frontend: ESLint + Prettier

**ESLint-Konfiguration (`eslint.config.js`):**
- TypeScript-Parser
- `eslint-plugin-svelte` mit Svelte 5-Regeln
- Empfohlene Regeln: `eslint:recommended`, `plugin:svelte/recommended`
- Strikte Regeln: `no-unused-vars`, `no-console` (warn)

**Prettier-Konfiguration (`.prettierrc`):**
- `prettier-plugin-svelte`
- Konsistente Formatierung: Tabs/Spaces, Semikolons, Quotes

**npm Scripts:**
```json
{
  "lint": "eslint . --ext .ts,.svelte",
  "lint:fix": "eslint . --ext .ts,.svelte --fix",
  "format": "prettier --write .",
  "format:check": "prettier --check ."
}
```

### 7.2 Backend: Clippy + rustfmt

**Clippy** (Rust-Linter):
- Konfiguration in `backend/clippy.toml` oder `Cargo.toml` `[lints]`
- Pedantic Warnungen aktiviert
- `cargo clippy --all-targets -- -D warnings`

**rustfmt** (Rust-Formatter):
- Standard `rustfmt.toml`
- `cargo fmt --check` in CI

---

## Phase 8: GitHub CI/CD Workflows

### 8.1 Workflow: Pull Request Checks (`.github/workflows/ci.yml`)

Trigger: `pull_request` auf `main`

**Jobs:**

| Job | Schritte |
|---|---|
| **frontend-lint** | Checkout → Node 22 Setup → `npm ci` → `npm run lint` → `npm run format:check` |
| **frontend-test** | Checkout → Node 22 Setup → `npm ci` → `npm run test:coverage` → Coverage Report als Comment |
| **frontend-build** | Checkout → Node 22 Setup → `npm ci` → `npm run build` (prüft ob Build fehlerfrei) |
| **backend-lint** | Checkout → Rust Toolchain → `cargo fmt --check` → `cargo clippy -- -D warnings` |
| **backend-test** | Checkout → Rust Toolchain → MariaDB Service Container → Migrationen → `cargo test` |
| **e2e** | Checkout → Docker Compose up → Playwright Install → `npm run test:e2e` → Test-Artefakte hochladen |

### 8.2 Workflow: Main Branch (`.github/workflows/main.yml`)

Trigger: `push` auf `main` (nach Merge)

- Führt dieselben Jobs aus wie der PR-Workflow
- Zusätzlich: Docker Images bauen (Validierung, noch kein Push zu Registry)

### 8.3 Concurrency

- PRs: `concurrency: { group: ci-${{ github.ref }}, cancel-in-progress: true }`
- Main: Kein Cancel, alle Runs werden durchgeführt

---

## Phase 9: README aktualisieren

### Inhalte der neuen README

1. **Projektbeschreibung** — Was ist LILLY, für wen, Zielgruppe
2. **Tech Stack** — Übersichtstabelle (Frontend, Backend, DB, Infra)
3. **Schnellstart** — `docker compose up` in 3 Schritten
4. **Demo-Zugangsdaten** — `demo@lilly.app` / `demo1234`
5. **Entwicklung** — Lokales Setup ohne Docker (Frontend + Backend einzeln starten)
6. **Projektstruktur** — Monorepo-Übersicht
7. **Testing** — Wie Tests ausgeführt werden (Unit, Integration, E2E)
8. **Linting** — ESLint, Prettier, Clippy, rustfmt
9. **CI/CD** — Beschreibung der GitHub Workflows
10. **Contributing** — Hinweise für Beitragende
11. **Lizenz** — GPLv3

---

## Dateiübersicht (was erstellt wird)

```
lilly/
├── .github/
│   ├── workflows/
│   │   ├── ci.yml                    # PR-Checks
│   │   └── main.yml                  # Main-Branch-Checks
│   ├── copilot-instructions.md       # (existiert)
│   └── instructions/                  # (existiert)
├── frontend/
│   ├── src/
│   │   ├── routes/
│   │   │   ├── +layout.svelte
│   │   │   ├── +page.svelte          # Redirect → /login
│   │   │   └── login/
│   │   │       └── +page.svelte      # Login-Seite
│   │   ├── lib/
│   │   │   ├── components/
│   │   │   │   └── layout/
│   │   │   │       └── TopBar.svelte
│   │   │   └── api/
│   │   │       └── auth.ts
│   │   ├── app.css
│   │   └── app.html
│   ├── tests/
│   │   ├── login-page.test.ts
│   │   ├── login-integration.test.ts
│   │   └── auth-api.test.ts
│   ├── e2e/
│   │   └── login.spec.ts
│   ├── static/
│   │   └── favicon.png
│   ├── Dockerfile
│   ├── package.json
│   ├── svelte.config.js
│   ├── vite.config.ts
│   ├── vitest.config.ts
│   ├── tsconfig.json
│   ├── playwright.config.ts
│   ├── eslint.config.js
│   └── .prettierrc
├── backend/
│   ├── src/
│   │   ├── main.rs
│   │   ├── config.rs
│   │   ├── error.rs
│   │   ├── routes/
│   │   │   ├── mod.rs
│   │   │   ├── auth.rs
│   │   │   └── health.rs
│   │   ├── models/
│   │   │   ├── mod.rs
│   │   │   └── user.rs
│   │   ├── db/
│   │   │   ├── mod.rs
│   │   │   └── users.rs
│   │   ├── auth/
│   │   │   ├── mod.rs
│   │   │   ├── jwt.rs
│   │   │   └── password.rs
│   │   └── services/
│   │       └── mod.rs
│   ├── migrations/
│   │   ├── 001_create_users.sql
│   │   └── 002_seed_demo_user.sql
│   ├── tests/
│   │   └── auth_integration.rs
│   ├── Cargo.toml
│   ├── Dockerfile
│   └── .sqlx/                        # SQLx offline query data
├── importer/                          # Platzhalter
│   ├── src/
│   │   └── main.rs
│   └── Cargo.toml
├── docker-compose.yml
├── Caddyfile
├── .env.example
├── .gitignore
├── LICENSE                            # (existiert)
└── README.md                          # Aktualisiert
```

---

## Abweichungen vom Architektur-Dokument

| Thema | Architektur-Dokument | Dieser Plan | Grund |
|---|---|---|---|
| Importer-Container | Eigener Docker Container | Nur Platzhalter-Verzeichnis | Kein Import für Login-Seite nötig |
| OAuth | Funktionale OAuth-Flows | Buttons vorhanden, nicht funktional | Erfordert externe Provider-Setup |
| Alle Tabellen | Vollständiges Schema | Nur `users`-Tabelle | Weitere Tabellen bei Bedarf in späteren Phasen |
| PWA / Service Worker | Vollständig konfiguriert | Nicht in Phase 1 | Login-Seite braucht keine Offline-Funktionalität |
| Paraglide.js i18n | Typ-sichere Übersetzungen | Nur Platzhalter-Struktur | Wird bei Implementierung weiterer Seiten vollständig eingerichtet |

---

*Ende des Implementierungsplans*
