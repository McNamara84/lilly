---
description: "Use when writing, modifying, or reviewing tests. Covers test strategy, coverage goals, and testing patterns for Svelte (Vitest) and Rust (#[cfg(test)])."
applyTo: ["**/*.test.ts", "**/*.test.js", "**/*.spec.ts", "**/*.spec.js", "**/tests/**", "**/*_test.rs"]
---
# Testing Guidelines

## Coverage Goal

Aim for **100% test coverage** on every new feature, bugfix, or refactor. This is the target even when hard to achieve — never settle for less without a documented reason. The absolute minimum acceptable threshold is 70%, but treat that as a failure case, not a goal.

## What to Test

- **All public functions and methods** — verify inputs, outputs, and side effects.
- **Edge cases**: empty inputs, boundary values, off-by-one errors, maximum sizes.
- **Error paths**: invalid input, network failures, missing data, unauthorized access.
- **State transitions**: verify correct state before and after operations.
- **Integration points**: API endpoints, database queries, component interactions.

## Frontend (Vitest + Testing Library)

- Use `@testing-library/svelte` for component tests — test behavior, not implementation.
- Query by accessible roles, labels, and text for assertions that validate the user experience.
- Use `data-testid` attributes when the selector must be stable and is not itself part of the test assertion (e.g., container elements, dynamic lists, layout anchors).
- Mock API calls with `vi.mock()` or MSW (Mock Service Worker).
- Test user interactions: clicks, form submissions, keyboard navigation.
- Verify rendered output, not internal component state.
- Test loading states, error states, and empty states — not just the happy path.

## E2E Tests (Playwright)

- Use `data-testid` attributes as the primary selector strategy for stable, refactor-proof element targeting.
- Add `data-testid` attributes to components during implementation — this is not test pollution, it is test infrastructure.
- Use accessible selectors (`getByRole`, `getByLabel`) only when the accessible property itself is what you are testing.
- Write Playwright tests with modern Playwright APIs: `expect(locator).toBeVisible()`, `locator.click()`, etc.
- Cover critical user flows end-to-end: login, navigation, form submissions, error scenarios.

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
