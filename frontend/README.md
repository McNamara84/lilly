# LILLY Frontend

The frontend for the LILLY project, built with **Svelte 5 / SvelteKit 3**, **Skeleton UI**, and **Tailwind CSS v4**.

For project documentation, setup instructions, and architecture details, see the [root README](../README.md).

## Development

Use Node.js **26.13.1 LTS** and npm **12.2.3**. SvelteKit configuration lives in
`vite.config.ts`; internal imports use the `#lib` package alias.

TypeScript **7.0.2** runs the Svelte and service-worker type checks. TypeScript
**6.0.3** remains under `typescript` for the JavaScript compiler API required by
SvelteKit and ESLint. The native compiler is installed as
`@typescript/native-preview: npm:typescript@^7.0.2`: this alias resolves to the stable
TypeScript release. It avoids the `@typescript/native` peer dependency reserved by
typescript-eslint for its experimental TypeScript 7.1 integration.

Use the npm scripts below to select the compiler explicitly; a bare `tsc` command
can resolve to either installed compiler.

```bash
npm ci
npm run dev          # Start dev server on http://localhost:5173
```

## Testing

Playwright is temporarily pinned to **1.66.0-alpha-2026-11-21** because the latest
stable release, 1.65.1, bundles a Firefox build that crashes when Axe and Playwright
both read ARIA label references. The pinned version includes the corrected Firefox
build 1578. See [the upstream diagnosis](https://github.com/microsoft/playwright/issues/44135#issuecomment-6918863177).
Return to a stable Playwright release once it includes the browser fix.

Install the browsers matching the lock file after installing or updating dependencies:

```bash
npm ci
npx playwright install
```

Start the isolated E2E stack from the `frontend` directory before running Playwright:

```bash
docker compose -f ../docker-compose.yml -f ../docker-compose.e2e.yml up -d --build --wait
```

```bash
npm run test             # Unit tests (Vitest)
npm run test:coverage    # Unit tests with coverage
npm run test:e2e         # Chromium E2E tests (requires Docker stack)
npm run test:e2e:all     # Chromium, Firefox, and WebKit
npm run test:e2e:mobile  # Mobile Chrome emulation
npm run test:e2e:ui      # Chromium in Playwright UI mode
```

## Linting & Formatting

```bash
npm run lint             # ESLint
npm run format:check     # Prettier
npm run check            # TypeScript 7: Svelte and service worker
npm run check:svelte     # TypeScript 7: Svelte and application code
npm run check:service-worker # TypeScript 7: service worker
npm run check:watch      # TypeScript 7: watch Svelte and application code
```
