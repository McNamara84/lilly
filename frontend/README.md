# LILLY Frontend

The frontend for the LILLY project, built with **Svelte 5 / SvelteKit**, **Skeleton UI**, and **Tailwind CSS v4**.

For project documentation, setup instructions, and architecture details, see the [root README](../README.md).

## Development

```bash
npm install
npm run dev          # Start dev server on http://localhost:5173
```

## Testing

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
npm run check            # Svelte type check
```
