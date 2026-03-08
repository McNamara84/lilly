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

- Aim for **close to 100% test coverage** wherever feasible. The minimum acceptable threshold is 70%, but always strive higher.
- Write unit tests for all business logic, utility functions, and components.
- Write integration tests for API endpoints, database queries, and critical user flows.
- Frontend: Use Vitest + Testing Library for component and unit tests.
- Backend: Use Rust's built-in test framework (`#[cfg(test)]`, `#[tokio::test]`).
- Test edge cases, error paths, and boundary conditions — not just the happy path.

## Code Review

- When points are raised in a code review, **carefully evaluate each one** before implementing changes.
- Determine whether the criticism is justified and whether the suggested change actually improves the code.
- Do not blindly apply all review suggestions — some may be subjective, context-dependent, or counterproductive.
- Prioritize: correctness > security > performance > readability > style.

## Security

- Follow OWASP Top 10 guidelines.
- Never store secrets in code or config files committed to Git.
- Validate all user input server-side.
- Use parameterized queries exclusively (SQLx prepared statements).
- Passwords hashed with argon2id.
- File uploads: validate MIME type, enforce size limits, re-encode images.
