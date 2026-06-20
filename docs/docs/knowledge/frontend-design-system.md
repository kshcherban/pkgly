# Frontend Design System

Pkgly's UI is a Vue 3 + Vuetify 3 application whose visual identity is driven by a **single source of truth** for color and a small set of **shared UI primitives**. This page documents both so contributors can change the look of the whole app from one place.

## Brand palette

Colors are derived from the Pkgly logo (`site/public/logo.svg`) — an orange + teal origami mark.

| Token | Value | Role |
| --- | --- | --- |
| `primary` | `#C95A00` (deep orange) | CTAs, links, active states. Tuned for WCAG AA on white. |
| `secondary` | `#145964` (deep teal) | Secondary surfaces. |
| `accent` | `#FFB70B` (amber) | Sparing highlights. |
| `success` | `#43A047` | Active / positive status. |
| `warning` | `#F9A825` | Shifted away from primary orange to stay distinct. |
| `error` | `#E53935` | Errors. |

### Where colors live (single source)

The canonical palette is **`site/src/theme/palette.ts`**. Three layers consume it:

1. `site/src/plugins/vuetify.ts` — Vuetify theme colors (imports `palette`).
2. `site/src/utils/themeTokens.ts` — runtime CSS-var applier (derives from `palette`).
3. `site/src/assets/styles/tokens.scss` — static `:root` CSS custom properties (mirror of the palette, resolves before the JS applier runs).

`site/src/assets/styles/theme.scss` is now a **thin SCSS bridge** — it maps legacy `$primary` / `$accent` SCSS names onto the canonical CSS custom properties. **No hex values live there.**

A guard test (`site/src/__tests__/themePalette.spec.ts`) asserts that `tokens.scss` stays in sync with `palette.ts`. **To change a color, update `palette.ts` and `tokens.scss`** — the test will fail until they agree.

## Spacing, radius, typography, elevation, motion

All non-color tokens are CSS custom properties in `tokens.scss` under the `--nr-*` prefix (spacing on a 4px base, radii, font sizes/weights, shadows, z-index, transitions). Prefer these tokens over hardcoded values in components.

## Shared UI primitives

Reusable components live in `site/src/components/ui/` and `site/src/components/layout/`. Prefer these over duplicating markup.

| Component | Path | Purpose |
| --- | --- | --- |
| `StatusChip` | `components/ui/StatusChip.vue` | Unified `Secured/Unsecured` + `Active/Inactive` badges. |
| `MonoValue` | `components/ui/MonoValue.vue` | Truncated hash/digest display with copy-to-clipboard. |
| `TableSkeleton` | `components/ui/TableSkeleton.vue` | Loading placeholder that mimics table rows. |
| `EmptyState` | `components/ui/EmptyState.vue` | Icon + title + message + optional action for empty collections. |
| `Breadcrumbs` | `components/layout/Breadcrumbs.vue` | Accessible breadcrumb nav (final segment is active). |
| `BrandMark` | `components/layout/BrandMark.vue` | Logo + wordmark lockup; horizontal (app bar) or stacked (login). |

### Conventions

- Components are **token-driven** (`var(--nr-*)`) so they adapt to the palette automatically.
- Each primitive has a co-located `__tests__/*.spec.ts` (Vitest + `@vue/test-utils`).
- When adding a new status/badge/tone, add a token to `tokens.scss` rather than a hardcoded color.
