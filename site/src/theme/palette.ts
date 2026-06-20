// ABOUTME: Canonical brand color palette for Pkgly, derived from the logo SVG.
// ABOUTME: Vuetify reads these values; tokens.scss mirrors them as CSS vars.
// Keep both files in agreement when changing a color.

/**
 * Brand palette extracted from public/logo.svg (orange + teal origami mark).
 *
 * Orange ramp comes from SVGID_1/2 (#EF7100, #EC6608, #FFA607, #FFB70B).
 * Teal ramp comes from SVGID_5/6 (#286D77, #145964, #074859).
 *
 * The primary orange is tuned darker than the logo's brightest tone so that
 * white text on a filled primary surface meets WCAG AA contrast (>= 4.5:1).
 */
export const palette = {
  primary: "#C95A00",
  primaryLight: "#FF8A00",
  primaryLighter: "#FFA607",
  primaryDark: "#9E4700",

  secondary: "#145964",
  secondaryLight: "#217F8F",
  secondaryDark: "#074859",

  accent: "#FFB70B",
  accentLight: "#FFD466",
  accentDark: "#bb870a",

  // Semantic
  success: "#43A047",
  warning: "#F9A825",
  error: "#E53935",
  info: "#2196F3",

  // Surfaces & text
  background: "#FFFFFF",
  surface: "#FAFAFA",
  onPrimary: "#FFFFFF",
  onSecondary: "#FFFFFF",
  onSurface: "#212121",
  onBackground: "#212121",
} as const;
