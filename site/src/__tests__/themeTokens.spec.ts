import { describe, expect, it } from "vitest";
import { themeTokens, applyThemeTokens } from "@/utils/themeTokens";

const expectedTokens = {
  "--nr-text-color": "rgba(0, 0, 0, 0.87)",
  "--nr-text-secondary": "rgba(0, 0, 0, 0.6)",
  "--text-secondary": "rgba(0, 0, 0, 0.6)",
  "--nr-background-primary": "#FFFFFF",
  "--nr-background-secondary": "#FAFAFA",
  "--nr-background-tertiary": "#F5F5F5",
  "--nr-border-color": "rgba(0, 0, 0, 0.12)",
  "--border-color": "rgba(0, 0, 0, 0.12)",
  "--nr-primary-color": "#1E88E5",
  "--nr-primary-color-strong": "#1565C0",
  "--nr-primary-color-soft": "#42A5F5",
  "--nr-accent-color": "#03A9F4",
  "--nr-accent-color-soft": "#4FC3F7",
  "--nr-success-color": "#43A047",
  "--error-color": "#E53935",
  "--nr-focus-ring": "rgba(30, 136, 229, 0.25)",
  "--nr-input-background": "#FFFFFF",
  "--nr-input-border": "rgba(0, 0, 0, 0.38)",
  "--nr-input-placeholder": "rgba(0, 0, 0, 0.38)",
  "--nr-table-row-hover": "rgba(30, 136, 229, 0.08)",
} as const;

describe("theme tokens", () => {
  it("matches the Material light theme design tokens", () => {
    expect(themeTokens).toEqual(expectedTokens);
  });

  it("applies CSS custom properties to the target element", () => {
    const target = document.createElement("div");

    applyThemeTokens(target);

    for (const [variable, value] of Object.entries(themeTokens)) {
      expect(target.style.getPropertyValue(variable)).toBe(value);
    }
  });
});
