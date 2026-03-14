# LILLY – Project Guidelines

LILLY (Listing Inventory for Lovely Little Yellowbacks) is a PWA for managing and trading German pulp fiction (Heftroman) collections.

## Tech Stack

- **Frontend**: Svelte 5 / SvelteKit with Skeleton UI + Tailwind CSS
- **Backend**: Rust / Axum REST API with SQLx (compile-time verified queries)
- **Database**: MariaDB 11.x
- **Infrastructure**: Docker Compose, Caddy v2 reverse proxy
- **Importer**: Rust CLI for wiki data import (Maddraxikon, Gruselroman-Wiki)

## Language

- All code, comments, variable names, function names, commit messages, and documentation in code must be written in **English**.
- The `docs/` folder contains **German-language** product specifications and planning documents. This is intentional — the primary audience and domain are German-speaking. These are not "documentation in code" and are exempt from the English rule.
- The user communicates in German — respond in German but never write code or comments in German.

## Architecture

- Planned monorepo structure: `frontend/`, `backend/`, `importer/`, `docs/` (only `docs/` exists during the planning phase)
- See `docs/architecture.md` for system design, request flow, and container setup.
- See `docs/requirements.md` for functional and non-functional requirements.
- See `docs/uxdesign.md`, `docs/design-tokens.json`, `docs/components.json`, `docs/screens.json` for UI/UX specs.
- Always consult the `docs/` folder before making architectural or design decisions.

## Conventions

### Frontend (Svelte 5 / SvelteKit)
- Use Svelte 5 Runes (`$state`, `$derived`, `$effect`) — not legacy stores.
- File-based routing in `src/routes/`.
- Components in `src/lib/components/`, API client in `src/lib/api/`.
- Skeleton UI components + Tailwind CSS for styling.
- Lucide Icons for icons.
- Paraglide.js for i18n (type-safe translations).
- Mobile-first responsive design.

### Backend (Rust / Axum)
- SQLx with compile-time SQL verification — no ORM.
- Tower middleware for cross-cutting concerns (rate limiting, CORS).
- Input validation with `serde` + `validator`.
- JWT authentication (access + refresh tokens).
- API endpoints under `/api/v1`.
- OpenAPI 3.1 documentation via `utoipa`.

### Database
- MariaDB with InnoDB engine.
- Migrations managed via SQLx.
- Always use parameterized queries (prepared statements).

## Testing

- Aim for **100% test coverage** on every new feature, bugfix, or refactor. The absolute minimum threshold is 70%, but always target 100%.
- Write unit tests for all business logic, utility functions, and components.
- Write integration tests for API endpoints, database queries, and critical user flows.
- Frontend: Use Vitest + Testing Library for component and unit tests.
- Frontend E2E: Use Playwright with `data-testid` as the primary selector strategy for stable element targeting.
- Backend: Use Rust's built-in test framework (`#[cfg(test)]`, `#[tokio::test]`).
- Test edge cases, error paths, and boundary conditions — not just the happy path.

## Code Style & Modernness

- Always use the **most modern, idiomatic features** of each language and framework.
- Svelte 5: Runes only (`$state`, `$derived`, `$effect`, `$props`) — never legacy stores or `$:` syntax.
- Rust: Use current edition idioms, modern error handling patterns, and latest stable features.
- TypeScript: Use modern syntax (satisfies, const assertions, template literal types where appropriate).
- Playwright: Use modern Playwright API patterns, prefer `data-testid` selectors for stability.
- When a newer, cleaner approach exists for a pattern, prefer it over legacy alternatives.

## Accessibility

- Accessibility is a **high priority** in this project — consider it in every new implementation.
- Use semantic HTML elements (`<nav>`, `<main>`, `<section>`, `<button>`, `<form>`, etc.) — avoid generic `<div>`/`<span>` when a semantic element exists.
- All interactive elements must be keyboard-accessible (`Tab`, `Enter`, `Escape`).
- Use proper ARIA attributes (`aria-label`, `aria-describedby`, `aria-invalid`, `role`) where semantic HTML alone is insufficient.
- All form inputs must have associated `<label>` elements.
- Images and icons must have `alt` text or `aria-label` (decorative icons: `aria-hidden="true"`).
- Ensure sufficient color contrast (WCAG 2.1 AA minimum).
- Support screen readers: logical heading hierarchy, focus management, live regions for dynamic content.

## Code Review

- When points are raised in a code review, **carefully evaluate each one** before implementing changes.
- Determine whether the criticism is justified and whether the suggested change actually improves the code.
- Verify the reviewer's claims — they may be factually incorrect. Check against docs, tests, or specs before acting.
- Do not blindly apply all review suggestions — some may be subjective, context-dependent, or counterproductive.
- Prioritize: correctness > security > performance > readability > style.

## Pre-Commit Checks

Before considering any implementation task complete, **always run the relevant linters and checks** to ensure CI will pass:

- **Backend (Rust)**: Run `cargo fmt --check` and `cargo clippy` — CI enforces both. Fix all issues before committing.
- **Frontend (Svelte/TS)**: Run `npm run lint`, `npx svelte-check`, and `npx vitest run` — ensure zero errors and all tests pass before committing.
- **Never skip these checks** — formatting or lint failures in CI are avoidable and waste review cycles.

## Security

- Follow OWASP Top 10 guidelines.
- Never store secrets in code or config files committed to Git.
- Validate all user input server-side.
- Use parameterized queries exclusively (SQLx prepared statements).
- Passwords hashed with argon2id.
- File uploads: validate MIME type, enforce size limits, re-encode images.
