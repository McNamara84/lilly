# LILLY – Listing Inventory for Lovely Little Yellowbacks

[![CI](https://github.com/McNamara84/lilly/actions/workflows/ci.yml/badge.svg)](https://github.com/McNamara84/lilly/actions/workflows/ci.yml)
[![codecov](https://codecov.io/gh/McNamara84/lilly/branch/main/graph/badge.svg)](https://codecov.io/gh/McNamara84/lilly)
[![License: GPL v3](https://img.shields.io/badge/License-GPLv3-blue.svg)](https://www.gnu.org/licenses/gpl-3.0)

![Svelte](https://img.shields.io/badge/Svelte_5-FF3E00?logo=svelte&logoColor=white)
![SvelteKit](https://img.shields.io/badge/SvelteKit-FF3E00?logo=svelte&logoColor=white)
![Tailwind CSS](https://img.shields.io/badge/Tailwind_CSS_4-06B6D4?logo=tailwindcss&logoColor=white)
![Rust](https://img.shields.io/badge/Rust-000000?logo=rust&logoColor=white)
![Axum](https://img.shields.io/badge/Axum-000000?logo=rust&logoColor=white)
![MariaDB](https://img.shields.io/badge/MariaDB_11-003545?logo=mariadb&logoColor=white)
![Docker](https://img.shields.io/badge/Docker-2496ED?logo=docker&logoColor=white)
![Caddy](https://img.shields.io/badge/Caddy_v2-1F88C0?logo=caddy&logoColor=white)

LILLY is an open-source web application (PWA) for managing and trading paperback novel collections in German-speaking countries. It is built for collectors of German *Heftromane* (also known as *Groschenromane* or *Groschenhefte* – serialized pulp fiction novellas) and provides a central platform for cataloging, showcasing, and trading issues.

> **Status:** Under active development – Login, registration, email verification, and dashboard are functional. Further features in progress.

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
|----------|------------------|
| Email    | `demo@lilly.app` |
| Password | `demo1234`       |

The demo user is only created when `ENABLE_DEMO_SEED=true` is set (disabled by default).

---

## Motivation

While generic book managers and general-purpose collector software exist, there is no specialized solution for the needs of *Heftroman* collectors. They have unique requirements around condition grading, series-based management, and the absence of ISBN numbers. LILLY fills this gap as a community project – no commercial business model, no ads, no commissions.

---

## Features

### Collection Management
- Add issues from available series to your personal collection
- Condition grading using the established collector scale (Z0–Z5)
- Mark issues as *Owned*, *Duplicate/Tradeable*, or *Wanted*
- Collection progress per series as progress bar and percentage
- Grid view of all issues in a series with color-coded ownership status
- Filter and sort by series, issue number, condition, title, and author
- Import/export collection data (CSV, JSON)

### Trading System
- Offer duplicate issues for trade
- Automatic matching: mutual matches between offers and wants are detected and reported
- Internal messaging system for arranging trades
- Deliberately **no** buy/sell system – LILLY is a pure trading platform

### Series Data and Import
- Initial series: **Maddrax – Die dunkle Zukunft der Erde** and **Geisterjäger John Sinclair**
- Master data (issue number, title, author, publication date) imported from fan wikis ([Maddraxikon](https://de.maddraxikon.com), [Gruselroman-Wiki](https://gruselroman-wiki.de))
- Regular sync via cronjob to automatically capture new issues
- Modular import system – additional series can be added with new adapters

### Community
- Public collector profiles with statistics
- Wishlists and trade lists can be shared publicly
- Upload your own photos per issue (condition documentation, special features)
- Comments and ratings on individual issues

### User Management
- Registration via email/password or OAuth (Google, GitHub)
- Profile with display name, avatar, and optional location
- Profile visibility (public/private) configurable
- GDPR-compliant: full account and data deletion supported

---

## Condition Grading Scale

The following scale is the established standard in the German *Heftroman* collector community:

| Grade | Label | Description |
|-------|-------|-------------|
| **Z0** | Mint | Unread, no signs of use, newsstand condition |
| **Z1** | Near Mint | Minimally read, barely visible signs of use |
| **Z2** | Good | Read, light signs of use, minor corner creases possible |
| **Z3** | Acceptable | Noticeable signs of use, creases, slight discoloration |
| **Z4** | Poor | Heavy signs of use, tears, stains, loose pages |
| **Z5** | Damaged | Severe damage, missing pages, water damage |

---

## Tech Stack

| Component | Technology |
|---|---|
| **Frontend** | Svelte 5 / SvelteKit (PWA) |
| **UI** | Skeleton UI + Tailwind CSS (Glassmorphism design) |
| **Backend** | Rust + Axum |
| **Database** | MariaDB 11.x |
| **DB Access** | SQLx (compile-time verified queries) |
| **Auth** | JWT + argon2id, OAuth2 (Google, GitHub) |
| **API** | REST, documented via OpenAPI 3.1 / Swagger |
| **Reverse Proxy** | Caddy v2 (automatic HTTPS) |
| **Wiki Importer** | Rust CLI (reqwest + scraper) |
| **Containerization** | Docker + Docker Compose |
| **i18n** | Paraglide.js |

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

The PWA is **offline-capable**: core collection management features remain available without an internet connection (Service Worker, local cache). The UI follows a **mobile-first** approach and is fully usable on all screen sizes.

---

## Project Structure

```
lilly/
├── frontend/              # SvelteKit PWA (Svelte 5, Skeleton UI, Tailwind CSS v4)
├── backend/               # Rust / Axum REST API
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
cd frontend
npx playwright install   # Install browsers (first time)
npm run test:e2e         # Run E2E tests (requires Docker stack running)
npm run test:e2e:ui      # Interactive UI mode
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
  - E2E: full Docker stack + Playwright tests

- **On Push to Main** (`.github/workflows/main.yml`):
  - All PR checks + Docker image build validation

---

## Roadmap

### Phase 1 – MVP
- Collection management for Maddrax and John Sinclair
- Data import from Maddraxikon and Gruselroman-Wiki
- Condition grading (Z0–Z5)
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

| Document | Contents |
|---|---|
| [requirements.md](docs/requirements.md) | Requirements catalog with all functional and non-functional requirements |
| [architecture.md](docs/architecture.md) | Architecture and design document (tech stack, database schema, API design, deployment) |
| [uxdesign.md](docs/uxdesign.md) | UI/UX concept (design philosophy, components, screens, responsive strategy) |
| [design-tokens.json](docs/design-tokens.json) | Machine-readable design tokens (colors, typography, spacing, animations) |
| [components.json](docs/components.json) | Machine-readable component specifications |
| [screens.json](docs/screens.json) | Machine-readable page structure and routing |

---

## License

This project is licensed under the [GNU General Public License v3.0](LICENSE).

---

## Author

**Holger Ehrmann** – Initiator and lead developer
