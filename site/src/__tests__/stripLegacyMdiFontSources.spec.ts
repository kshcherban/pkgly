import { describe, expect, it } from "vitest";

import { stripLegacyMdiFontSources } from "../../vite/plugins/stripLegacyMdiFontSources";

describe("stripLegacyMdiFontSources", () => {
  it("removes .eot and .ttf entries from @font-face src", () => {
    const input = `
@font-face {
  font-family: "Material Design Icons";
  src: url("../fonts/materialdesignicons-webfont.eot?v=7.4.47");
  src: url("../fonts/materialdesignicons-webfont.eot?#iefix&v=7.4.47") format("embedded-opentype"),
    url("../fonts/materialdesignicons-webfont.woff2?v=7.4.47") format("woff2"),
    url("../fonts/materialdesignicons-webfont.woff?v=7.4.47") format("woff"),
    url("../fonts/materialdesignicons-webfont.ttf?v=7.4.47") format("truetype");
  font-weight: normal;
  font-style: normal;
}

.mdi:before { content: "\\F000"; }
`;

    const output = stripLegacyMdiFontSources(input);

    expect(output).not.toMatch(/\.eot\b/);
    expect(output).not.toMatch(/embedded-opentype/);
    expect(output).not.toMatch(/\.ttf\b/);
    expect(output).not.toMatch(/truetype/);

    expect(output).toMatch(/\.woff2\b/);
    expect(output).toMatch(/\.woff\b/);
    expect(output).toMatch(/\.mdi:before/);
  });
});

