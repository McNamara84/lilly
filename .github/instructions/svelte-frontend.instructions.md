---
description: "Use when writing or modifying Svelte components, SvelteKit routes, frontend API clients, or Tailwind styles for the LILLY frontend."
applyTo: "frontend/**"
---
# Svelte 5 / SvelteKit Frontend Guidelines

## Svelte 5 Runes

- Use `$state()` for reactive state — never use legacy `writable`/`readable` stores.
- Use `$derived()` for computed values — replaces `$:` reactive declarations.
- Use `$effect()` for side effects — replaces `$:` reactive statements.
- Use `$props()` to declare component props.
- Use `$bindable()` for two-way bindable props.

## Component Structure

- Place reusable components in `src/lib/components/`.
- Use Skeleton UI components as the base — extend with Tailwind utility classes.
- Keep components small and focused on a single responsibility.
- Use Lucide Icons (`lucide-svelte`) for all icons.

## Styling

- Tailwind CSS utility classes — no custom CSS files unless absolutely necessary.
- Mobile-first responsive design: start with mobile layout, add `sm:`, `md:`, `lg:` breakpoints.
- Use CSS custom properties from `docs/design-tokens.json` for theme colors.
- Support both light and dark mode via Skeleton's theme system.

## Routing & Data Loading

- File-based routing in `src/routes/`.
- Use `+page.ts` / `+page.server.ts` for data loading (load functions).
- Use `+layout.ts` / `+layout.svelte` for shared layouts.
- Handle loading and error states in every route.

## API Client

- Centralize API calls in `src/lib/api/`.
- Use typed request/response interfaces.
- Handle errors consistently — never swallow errors silently.

## i18n

- Use Paraglide.js for all user-facing strings — no hardcoded text.
- Translation keys should be descriptive: `collection.addIssue`, not `btn1`.
