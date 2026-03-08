---
description: "Use when writing or modifying Rust code for the Axum backend API or the wiki importer CLI. Covers API patterns, SQLx queries, authentication, and error handling."
applyTo: ["backend/**", "importer/**"]
---
# Rust / Axum Backend Guidelines

## API Design

- All endpoints under `/api/v1`.
- Use Axum extractors for request parsing (`Json`, `Path`, `Query`).
- Return consistent JSON responses with appropriate HTTP status codes.
- Document endpoints with `utoipa` annotations for OpenAPI 3.1.

## Database (SQLx + MariaDB)

- Use `sqlx::query!` / `sqlx::query_as!` macros for compile-time SQL verification.
- Avoid building SQL strings dynamically; always use SQLx parameter binding / prepared statements (`query!`, `query_as!`).
- Place migrations in `backend/migrations/` managed by SQLx CLI.
- Use transactions for multi-step operations that must be atomic.

## Authentication

- JWT access tokens (15 min) + refresh tokens (30 days, httpOnly cookie).
- Hash passwords with `argon2id`.
- Validate JWT in middleware — protect routes via Tower layers.
- Support OAuth2 flows for Google and GitHub.

## Error Handling

- Define a unified `AppError` type that implements `IntoResponse`.
- Map internal errors to user-safe messages — never expose stack traces or DB details.
- Use `thiserror` for library-style error enums.
- Log errors with `tracing` — structured logging with context.

## Input Validation

- Validate all incoming data with `serde` deserialization + `validator` crate.
- Reject invalid input early — return 400 with a descriptive error message.
- File uploads: validate MIME type (JPEG/PNG/WebP only), enforce 5 MB limit, re-encode images.

## Project Structure

```
backend/src/
├── main.rs          # Server startup, router, middleware
├── routes/          # Handler functions grouped by domain
├── models/          # Request/response DTOs, DB row types
├── db/              # SQLx query functions
├── auth/            # JWT, OAuth, password hashing
└── services/        # Business logic (matching, trades)
```

## Importer

- Implement `WikiAdapter` trait for each data source.
- Use `reqwest` for HTTP requests, `scraper` for HTML parsing.
- Handle rate limiting gracefully — respect source wiki's robots.txt.
- Log import progress and errors with `tracing`.
