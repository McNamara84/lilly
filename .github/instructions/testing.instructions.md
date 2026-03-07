---
description: "Use when writing, modifying, or reviewing tests. Covers test strategy, coverage goals, and testing patterns for Svelte (Vitest) and Rust (#[cfg(test)])."
applyTo: ["**/*.test.ts", "**/*.test.js", "**/*.spec.ts", "**/*.spec.js", "**/tests/**", "**/*_test.rs"]
---
# Testing Guidelines

## Coverage Goal

Aim for **close to 100% test coverage** wherever feasible. The minimum acceptable threshold is 70%, but always push higher. Every new feature, bugfix, or refactor should include corresponding tests.

## What to Test

- **All public functions and methods** — verify inputs, outputs, and side effects.
- **Edge cases**: empty inputs, boundary values, off-by-one errors, maximum sizes.
- **Error paths**: invalid input, network failures, missing data, unauthorized access.
- **State transitions**: verify correct state before and after operations.
- **Integration points**: API endpoints, database queries, component interactions.

## Frontend (Vitest + Testing Library)

- Use `@testing-library/svelte` for component tests — test behavior, not implementation.
- Query by accessible roles, labels, and text — avoid test IDs when possible.
- Mock API calls with `vi.mock()` or MSW (Mock Service Worker).
- Test user interactions: clicks, form submissions, keyboard navigation.
- Verify rendered output, not internal component state.
- Test loading states, error states, and empty states — not just the happy path.

## Backend (Rust)

- Use `#[cfg(test)]` modules co-located with the code they test.
- Use `#[tokio::test]` for async tests.
- Use `sqlx::test` with test fixtures for database integration tests.
- Test request validation: missing fields, wrong types, out-of-range values.
- Test authorization: verify that protected endpoints reject unauthenticated requests.
- Test response shapes: status codes, JSON structure, error messages.

## Patterns

- **Arrange-Act-Assert**: Structure every test clearly.
- **One assertion per behavior**: Each test should verify one specific behavior.
- **Descriptive test names**: `test_login_with_invalid_email_returns_400`, not `test_login_2`.
- **No test interdependence**: Tests must run in any order and in isolation.
- **No sleeping**: Use proper async waiting, not `sleep()` or `setTimeout()`.
