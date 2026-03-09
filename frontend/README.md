# LILLY Frontend

The frontend for the LILLY project, built with **Svelte 5 / SvelteKit**, **Skeleton UI v3**, and **Tailwind CSS v4**.

For project documentation, setup instructions, and architecture details, see the [root README](../README.md).

## Development

```bash
npm install
npm run dev          # Start dev server on http://localhost:5173
```

## Testing

```bash
npm run test             # Unit tests (Vitest)
npm run test:coverage    # Unit tests with coverage
npm run test:e2e         # E2E tests (Playwright, requires Docker stack)
```

## Linting & Formatting

```bash
npm run lint             # ESLint
npm run format:check     # Prettier
npm run check            # Svelte type check
```
