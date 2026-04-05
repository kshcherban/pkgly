import type { Plugin } from "vite";

const mdiCssPathRegex =
  /[\\/]@mdi[\\/]font[\\/]css[\\/]materialdesignicons\.css$/;

function stripQuery(id: string): string {
  const queryIndex = id.indexOf("?");
  return queryIndex === -1 ? id : id.slice(0, queryIndex);
}

function isLegacyMdiFontSource(source: string): boolean {
  return source.includes(".eot") || source.includes(".ttf");
}

function stripLegacyFontSourcesFromFontFaceBlock(block: string): string {
  return block.replace(/src:\s*([^;]+);/g, (_match, sourcesRaw: string) => {
    const sources = sourcesRaw
      .split(",")
      .map((source) => source.trim())
      .filter((source) => source.length > 0);

    const filteredSources = sources.filter(
      (source) => !isLegacyMdiFontSource(source),
    );

    if (filteredSources.length === 0) {
      return "";
    }

    return `src: ${filteredSources.join(", ")};`;
  });
}

/**
 * Removes `.eot` (IE) and `.ttf` fallback sources from the MDI `@font-face`.
 *
 * The upstream `@mdi/font` CSS includes these for legacy browser support, but
 * Vite will still bundle/copy them into `dist/` even if our supported browsers
 * will never request them. Stripping the references prevents those assets from
 * being emitted.
 */
export function stripLegacyMdiFontSources(css: string): string {
  return css.replace(/@font-face\s*{[^}]*}/g, (fontFaceBlock) =>
    stripLegacyFontSourcesFromFontFaceBlock(fontFaceBlock),
  );
}

export function stripLegacyMdiFontSourcesPlugin(): Plugin {
  return {
    name: "strip-legacy-mdi-font-sources",
    enforce: "pre",
    transform(code, id) {
      const normalizedId = stripQuery(id);
      if (!mdiCssPathRegex.test(normalizedId)) {
        return;
      }

      const transformed = stripLegacyMdiFontSources(code);
      if (transformed === code) {
        return;
      }

      return { code: transformed, map: null };
    },
  };
}

