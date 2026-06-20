// ABOUTME: Verifies the Vuetify palette values stay mirrored in CSS tokens.
// ABOUTME: Keeps the duplicated theme constants honest without runtime token code.
import { readFile } from "node:fs/promises";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { palette } from "@/theme/palette";

const tokensPath = join(process.cwd(), "src", "assets", "styles", "tokens.scss");

const cssVars = {
  "--nr-primary": palette.primary,
  "--nr-primary-light": palette.primaryLight,
  "--nr-primary-lighter": palette.primaryLighter,
  "--nr-primary-dark": palette.primaryDark,
  "--nr-secondary": palette.secondary,
  "--nr-secondary-light": palette.secondaryLight,
  "--nr-secondary-dark": palette.secondaryDark,
  "--nr-accent": palette.accent,
  "--nr-accent-light": palette.accentLight,
  "--nr-accent-dark": palette.accentDark,
  "--nr-success": palette.success,
  "--nr-warning": palette.warning,
  "--nr-error": palette.error,
  "--nr-info": palette.info,
  "--nr-background": palette.background,
  "--nr-surface": palette.surface,
  "--nr-on-primary": palette.onPrimary,
  "--nr-on-secondary": palette.onSecondary,
  "--nr-on-surface": palette.onSurface,
  "--nr-on-background": palette.onBackground,
} as const;

describe("brand palette", () => {
  it("matches the mirrored CSS custom properties", async () => {
    const tokens = await readFile(tokensPath, "utf-8");

    for (const [name, value] of Object.entries(cssVars)) {
      expect(tokens, `${name} should equal ${value}`).toContain(`${name}: ${value};`);
    }
  });
});
