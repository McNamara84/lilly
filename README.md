# LILLY – Listing Inventory for Lovely Little Yellowbacks

[![CI](https://github.com/McNamara84/lilly/actions/workflows/ci.yml/badge.svg)](https://github.com/McNamara84/lilly/actions/workflows/ci.yml)
[![codecov](https://codecov.io/gh/McNamara84/lilly/branch/main/graph/badge.svg)](https://codecov.io/gh/McNamara84/lilly)
[![License: GPL v3](https://img.shields.io/badge/License-GPLv3-blue.svg)](https://www.gnu.org/licenses/gpl-3.0)

![Svelte](https://img.shields.io/badge/Svelte_5-FF3E00?logo=svelte&logoColor=white)
![SvelteKit](https://img.shields.io/badge/SvelteKit-FF3E00?logo=svelte&logoColor=white)
![Tailwind CSS](https://img.shields.io/badge/Tailwind_CSS_4-06B6D4?logo=tailwindcss&logoColor=white)
![Rust](https://img.shields.io/badge/Rust-000000?logo=rust&logoColor=white)
![Axum](https://img.shields.io/badge/Axum-000000?logo=rust&logoColor=white)
![MariaDB](https://img.shields.io/badge/MariaDB_12.3-003545?logo=mariadb&logoColor=white)
![Docker](https://img.shields.io/badge/Docker-2496ED?logo=docker&logoColor=white)
![Caddy](https://img.shields.io/badge/Caddy_v2-1F88C0?logo=caddy&logoColor=white)

LILLY is an open-source web application (PWA) for managing and trading paperback novel collections in German-speaking countries. It is built for collectors of German _Heftromane_ (also known as _Groschenromane_ or _Groschenhefte_ – serialized pulp fiction novellas) and provides a central platform for cataloging, showcasing, and trading issues.

> **Status:** Under active development – Login, collection management with multiple editions, reciprocal trade matching, proposals, notifications, trade-scoped messaging, two-party trade completion, and revocable GDPR account erasure are functional.

---

## Quick Start

### Prerequisites

- [Docker](https://docs.docker.com/get-docker/) and [Docker Compose](https://docs.docker.com/compose/install/)

### Run with Docker

```bash
# 1. Clone the repository
git clone https://github.com/McNamara84/lilly.git
cd lilly

# 2. Create environment file
cp .env.example .env

# 3. Start the application
docker compose up -d --build

# 4. Open in browser
# http://localhost
```

### Demo Credentials (Development Only)

> **Warning:** These credentials are publicly known. Never use `ENABLE_DEMO_SEED=true` in production.
> Before exposing the service publicly, ensure demo seeding is disabled and remove any demo accounts.

| Field    | Value            |
| -------- | ---------------- |
| Email    | `demo@lilly.app` |
| Password | `demo1234`       |

The demo user and a minimal deterministic catalogue fixture are only created when
`ENABLE_DEMO_SEED=true` is set (disabled by default). This fixture keeps local demos and E2E tests
independent from live wiki availability.

---

## Motivation

While generic book managers and general-purpose collector software exist, there is no specialized solution for the needs of _Heftroman_ collectors. They have unique requirements around condition grading, series-based management, and the absence of ISBN numbers. LILLY fills this gap as a community project – no commercial business model, no ads, no commissions.

---

## Features

### Collection Management

- Add issues from available series to your personal collection
- Condition grading using the established collector scale (Z0–Z4)
- Mark issues as _Owned_, _Duplicate/Tradeable_, or _Wanted_
- Track multiple physical copies and optional edition labels for the same issue independently
- Collection progress per series as progress bar and percentage
- Grid view of all issues in a series with color-coded ownership status
- Filter and sort by series, issue number, condition, title, and author
- Import/export collection data (CSV, JSON)

### Trading System

- Offer duplicate issues for trade
- Maintain a private wishlist from missing issues, including idempotent bulk selection
- Keep offers and wishes synchronized automatically with collection status changes
- Receive reciprocal matches when both collectors can fulfil each other's wishes
- Select items, propose or accept a trade, and coordinate it in a private message thread
- Confirm receipt from both sides to transfer the agreed copies atomically between collections
- Review completed and cancelled trades without losing their private message threads
- See deduplicated match, proposal, acceptance, completion, cancellation, and message notifications
- Deliberately **no** buy/sell system – LILLY is a pure trading platform

### Series Data and Import

- Initial series: **Maddrax – Die dunkle Zukunft der Erde** and **Geisterjäger John Sinclair**
- Master data (issue number, title, author, publication date) imported from fan wikis ([Maddraxikon](https://de.maddraxikon.com), [Gruselroman-Wiki](https://gruselroman-wiki.de))
- Complete, idempotent synchronization via the timezone-aware backend scheduler to capture new issues and improved wiki metadata
- Persistent progress with created/updated/unchanged/skipped/failed counters, cancellation and linked recovery runs
- Modular import system – additional series can be added with new adapters

### Community

- Editable collector profiles with privacy-aware avatars, locations, and per-series statistics
- Wishlists and trade lists can be shared publicly
- Upload your own photos per issue (condition documentation, special features)
- Comments and ratings on individual issues

### User Management

- Registration via email/password or OAuth Authorization Code + PKCE (Google, GitHub)
- Explicit, versioned privacy consent stored atomically with every new account
- Secure account linking: matching provider emails never link or sign in automatically
- Editable profile with display name, validated avatar upload, and optional coarse location
- Profile visibility (public/private) configurable
- GDPR-compliant: full account and data deletion supported

---

## Condition Grading Scale

The following scale is the established standard in the German _Heftroman_ collector community:

| Grade  | Label           | Description                                                                |
| ------ | --------------- | -------------------------------------------------------------------------- |
| **Z0** | Mint            | Freshly printed, no defects, white interior pages                          |
| **Z1** | Near Mint       | Minimal signs of use, no tears or cover markings                           |
| **Z2** | Good            | Normal signs of use, small edge tears or a light reading roll              |
| **Z3** | Damaged         | Larger tears, strong reading roll, darkened or stained pages               |
| **Z4** | Heavily damaged | Torn or marked cover, tattered appearance, possibly loose or missing pages |

---

## Tech Stack

| Component            | Technology                                        |
| -------------------- | ------------------------------------------------- |
| **Frontend**         | Svelte 5 / SvelteKit (PWA)                        |
| **UI**               | Skeleton UI + Tailwind CSS (Glassmorphism design) |
| **Backend**          | Rust + Axum                                       |
| **Database**         | MariaDB 12.3 LTS                                  |
| **DB Access**        | SQLx (compile-time verified queries)              |
| **Auth**             | JWT + argon2id, OAuth2 (Google, GitHub)           |
| **API**              | REST, documented via OpenAPI 3.1 / Swagger        |
| **Reverse Proxy**    | Caddy v2 (automatic HTTPS)                        |
| **Wiki Importer**    | Rust CLI (reqwest + scraper)                      |
| **Containerization** | Docker + Docker Compose                           |
| **i18n**             | Paraglide.js                                      |

---

## Architecture

```
┌──────────────────────────────────────────────────────────────┐
│                      Docker Host (VPS)                       │
│                                                              │
│  ┌────────────┐    ┌────────────┐    ┌──────────────┐        │
│  │   Caddy    │    │  SvelteKit │    │  Rust / Axum │        │
│  │  (Reverse  │────│  (Frontend │    │   (Backend)  │        │
│  │   Proxy)   │    │   SSR/PWA) │────│  REST API    │        │
│  └────────────┘    └────────────┘    └──────┬───────┘        │
│       │                                     │                │
│       │  Static Files          ┌────────────┘─┐              │
│       └────────────────────────┤   MariaDB    │              │
│  ┌────────────┐                │   11.x       │              │
│  │   /media   │                └──────────────┘              │
│  │  (Volume)  │                                              │
│  └────────────┘  ┌────────────────────────┐                  │
│                  │   Wiki-Importer (Cron) │                  │
│                  │   Rust CLI → MariaDB   │                  │
│                  └────────────────────────┘                  │
└──────────────────────────────────────────────────────────────┘
```

The application is designed for **self-hosting** on your own server or VPS. All components run in Docker containers, orchestrated via Docker Compose. Caddy provides automatic HTTPS via Let's Encrypt.

The installable PWA is **offline-capable**: its service worker caches only the app shell and shared reference covers, while user-scoped IndexedDB stores the catalogue, personal collection, durable mutation queue, and explicit conflicts. Offline creates and edits synchronize idempotently after reconnect; logout removes that user's private local data. Photos, messaging, and live trade matching remain online-only. The UI follows a **mobile-first** approach and is fully usable on all screen sizes.

---

## Project Structure

```
lilly/
├── frontend/              # SvelteKit PWA (Svelte 5, Skeleton UI, Tailwind CSS v4)
├── backend/               # Rust / Axum REST API
├── importer-core/         # Source-independent adapter contract and validation
├── importer-adapters/     # Built-in source parsers and deterministic fixtures
├── importer/              # Wiki data importer CLI (placeholder)
├── docs/                  # Planning documents (German)
├── docker-compose.yml     # Full stack orchestration
├── Caddyfile              # Reverse proxy configuration
└── .env.example           # Environment template
```

---

## Development

### Frontend

```bash
cd frontend
npm install
npm run dev          # Start dev server on http://localhost:5173
```

### Backend

Requires [Rust](https://rustup.rs/) and a running MariaDB instance.

```bash
cd backend
cargo run            # Start API server on http://localhost:8080
```

### OAuth and privacy consent

OAuth providers are optional and are enabled only when both values for that provider are set.
Register these exact callback URLs at the providers, using the public `APP_BASE_URL` of the
deployment:

- `${APP_BASE_URL}/api/v1/auth/oauth/google/callback`
- `${APP_BASE_URL}/api/v1/auth/oauth/github/callback`

```dotenv
APP_BASE_URL=https://lilly.example
COOKIE_SECURE=true
GOOGLE_OAUTH_CLIENT_ID=
GOOGLE_OAUTH_CLIENT_SECRET=
GITHUB_OAUTH_CLIENT_ID=
GITHUB_OAUTH_CLIENT_SECRET=
PRIVACY_POLICY_VERSION=2026-08-14
```

Google uses `openid email profile`; GitHub uses `read:user user:email`. LILLY accepts only a
verified provider email, never persists provider access tokens, and derives callbacks only from the
validated application origin. Increase `PRIVACY_POLICY_VERSION` whenever the registration-relevant
privacy text changes. The UI then rejects stale submissions and asks for fresh consent. Production
deployments require HTTPS and `COOKIE_SECURE=true`.

If a new provider identity returns an email already used by LILLY, the application creates only a
ten-minute linking request. The person must first authenticate the matching existing account and
then explicitly confirm the link; email equality alone never creates a session or links accounts.

### Password recovery and rate limiting

Verified password accounts can request a reset from `/forgot-password`. LILLY always returns the
same public response for existing, unknown, unverified and OAuth-only addresses. Reset links are
single-use, expire after one hour by default and are stored only as SHA-256 hashes. Completing a
reset revokes all refresh sessions atomically; an already issued access token expires after its
configured short TTL.

The backend applies a central in-memory sliding-window limiter to public and authenticated API
traffic, with tighter policies for registration, login, verification mail, OAuth, refresh and both
password-reset steps. A rejected request returns HTTP 429, a `Retry-After` header and a matching
`retry_after_seconds` JSON value. Configure limits as `MAX_REQUESTS/WINDOW_SECONDS` and trust
forwarding headers only from explicit proxy networks:

```dotenv
PASSWORD_RESET_TTL_SECONDS=3600
TRUSTED_PROXY_CIDRS=172.16.0.0/12
RATE_LIMIT_PASSWORD_RESET_REQUEST=5/900
RATE_LIMIT_PASSWORD_RESET_CONFIRM=10/900
RATE_LIMIT_PUBLIC_API=120/60
RATE_LIMIT_AUTHENTICATED_API=600/60
```

The limiter is intentionally process-local for the single-backend MVP deployment. A future
multi-instance deployment must replace it with a shared store.

The bootstrap account configured through `ADMIN_EMAIL` must already exist. Its address is
trimmed, lowercased and validated; a real promotion is written atomically with a retained
role-change audit event. Additional existing users can be promoted without supplying or
handling their password:

```bash
cd backend
cargo run -- admin promote --email user@example.org
```

The command is idempotent. Its exit codes are 0 for a promotion, 4 when the account already
is an admin, 2 for invalid input, 3 for an unknown account and 1 for database failures.
Existing access tokens keep their embedded role until expiry;
refreshing a session loads the current role from MariaDB and issues an updated access token.

### Automatic wiki imports

The backend can synchronize the MVP series through the same import service used by the admin UI. Scheduled imports are disabled by default so that the initial full imports can be reviewed before automation is enabled.

```dotenv
IMPORT_SCHEDULER_ENABLED=true
IMPORT_SCHEDULE=0 10 6 * * Sat *
IMPORT_TIMEZONE=Europe/Berlin
IMPORT_SCHEDULED_ADAPTERS=maddrax,john-sinclair
```

With these settings, Maddrax and the regular first edition of John Sinclair are fully compared with their authoritative wiki every Saturday at 06:10 local German time. Each source issue is classified as created, updated, unchanged, skipped or failed; existing local covers are not downloaded again, while missing local covers are recovered even when the bibliographic metadata is unchanged. The IANA timezone keeps the local execution time stable across daylight-saving changes. After a restart, the backend reserves at most the latest missed weekly slot; `(adapter, scheduled_for)` is unique, so repeated starts cannot create the same scheduled job twice. Scheduler state and the next run are visible to administrators on the import page.

The start request returns a persistent job with HTTP 202 before wiki access begins. Administrators can cancel active jobs; the first cancelling administrator and timestamp are stored atomically with the job. Jobs interrupted by a backend restart remain visible and can be retried as linked, idempotent full scans. The detail page polls MariaDB-backed progress every three seconds and exposes record-level error context.

For production rollout, first leave `IMPORT_SCHEDULER_ENABLED=false`, run both initial imports manually, review their complete job-specific result lists and pinned reference samples, acknowledge non-blocking warnings where appropriate, and publish each series from that import review. Activation and later deactivation are audited with the acting administrator; blocking or incomplete results cannot be published. Run an unchanged second synchronization before enabling the scheduler. See [Import sources and mapping contract](docs/import-sources.md) for source identities, mappings and recovery behavior. The source-independent contract is isolated from built-in implementations; see [Adding an import adapter](docs/adding-import-adapter.md) for the required offline contract and persistence tests.

### Personal issue photos

Personal collection photos use the persistent `media_data` volume but are not exposed as static
files. Caddy serves only imported reference covers below `/media/covers/`; personal photos pass
through the backend's ownership and collection-privacy checks. JPEG, PNG and WebP inputs are
decoded, orientation-corrected, stripped of metadata and stored as normalised JPEG derivatives.
The production defaults can be restricted through these settings:

```dotenv
PHOTO_MAX_UPLOAD_BYTES=5242880
PHOTO_MAX_COUNT=4
PHOTO_MAX_EDGE=2048
PHOTO_MAX_SOURCE_DIMENSION=10000
PHOTO_MAX_SOURCE_PIXELS=40000000
PHOTO_JPEG_QUALITY=85
```

`PHOTO_MAX_COUNT` remains fixed at four for the MVP schema. Failed file deletions are persisted and
retried during storage reconciliation after a backend restart.

---

## Testing

### Frontend Unit Tests (Vitest + Testing Library)

```bash
cd frontend
npm run test             # Run once
npm run test:watch       # Watch mode
npm run test:coverage    # With coverage report
```

### Frontend E2E Tests (Playwright)

```bash
# From the repository root, start the isolated E2E stack:
docker compose -f docker-compose.yml -f docker-compose.e2e.yml up -d --build --wait

cd frontend
npx playwright install   # Install browsers (first time)
npm run test:e2e         # Chromium only (requires Docker stack running)
npm run test:e2e:all     # Chromium, Firefox, and WebKit
npm run test:e2e:mobile  # Mobile Chrome emulation
npm run test:e2e:ui      # Chromium in interactive UI mode
npm run test:lighthouse  # Five mobile runs for /privacy and authenticated /collection

# From the repository root after the tests:
cd ..
docker compose -f docker-compose.yml -f docker-compose.e2e.yml down
```

### Backend Tests (Rust)

```bash
cd backend
cargo test               # Run all tests
```

---

## Linting & Formatting

### Frontend

```bash
cd frontend
npm run lint             # ESLint check
npm run lint:fix         # ESLint auto-fix
npm run format:check     # Prettier check
npm run format           # Prettier auto-format
npm run check            # Svelte type check
```

### Backend

```bash
cd backend
cargo fmt --check        # rustfmt check
cargo clippy -- -D warnings  # Clippy lint
```

---

## CI/CD

GitHub Actions workflows run automatically:

- **On Pull Requests** (`.github/workflows/ci.yml`):
  - Frontend: lint, format check, type check, unit tests with coverage, build
  - Backend: rustfmt, clippy, unit tests (with MariaDB service)
  - E2E: full Docker stack + Chromium Playwright tests and five-run Lighthouse median gates

- **On Push to Main** (`.github/workflows/main.yml`):
  - All PR checks + parallel Chromium, Firefox, and WebKit E2E jobs
  - Docker image build validation

---

## Roadmap

### Phase 1 – MVP

- Collection management for Maddrax and John Sinclair
- Data import from Maddraxikon and Gruselroman-Wiki
- Condition grading (Z0–Z4)
- User registration (email + OAuth)
- Trade matching and messaging system
- Public profiles and statistics
- Photo upload
- PWA with offline basics

### Phase 2 – Expansion

- Additional series (e.g. Perry Rhodan, Professor Zamorra, Ren Dhark)
- Rating system for trade partners
- Aggregated community statistics
- Push notifications
- English UI

### Phase 3 – Vision

- Ring trade algorithm (A→B→C→A)
- Barcode/cover scan for quick cataloging
- International pulp fiction series
- Collector events and trade fair calendar

---

## Documentation

Detailed planning documents are located in the [`docs/`](docs/) folder:

| Document                                                    | Contents                                                                               |
| ----------------------------------------------------------- | -------------------------------------------------------------------------------------- |
| [requirements.md](docs/requirements.md)                     | Requirements catalog with all functional and non-functional requirements               |
| [architecture.md](docs/architecture.md)                     | Architecture and design document (tech stack, database schema, API design, deployment) |
| [uxdesign.md](docs/uxdesign.md)                             | UI/UX concept (design philosophy, components, screens, responsive strategy)            |
| [design-tokens.json](docs/design-tokens.json)               | Machine-readable design tokens (colors, typography, spacing, animations)               |
| [components.json](docs/components.json)                     | Machine-readable component specifications                                              |
| [screens.json](docs/screens.json)                           | Machine-readable page structure and routing                                            |
| [trading.md](docs/trading.md)                               | Reciprocal matching, trade workflow, privacy, notifications, and message retention     |
| [privacy-data-inventory.md](docs/privacy-data-inventory.md) | Personal-data inventory, erasure/anonymisation rules, backups, and verification        |

---

## License

This project is licensed under the [GNU General Public License v3.0](LICENSE).

---

## Author

**Holger Ehrmann** – Initiator and lead developer
