// ABOUTME: Tests for truncateMiddle / isHashLike helpers.
import { describe, expect, it } from "vitest";
import { truncateMiddle, isHashLike } from "@/utils/truncateValue";

describe("truncateMiddle", () => {
  it("returns short strings unchanged", () => {
    expect(truncateMiddle("latest")).toBe("latest");
    expect(truncateMiddle("1.2.3")).toBe("1.2.3");
  });

  it("returns empty input unchanged", () => {
    expect(truncateMiddle("")).toBe("");
  });

  it("truncates a sha256 digest in the middle", () => {
    const digest = "sha256:1090bc3a8ccfb0b55f78a494d76f8d603434f7e4553543d6e807bc7bd6bbd17f";
    const result = truncateMiddle(digest);
    expect(result.length).toBeLessThan(digest.length);
    expect(result).toContain("…");
    // head=12 keeps the prefix including the sha256: scheme
    expect(result.startsWith("sha256:1090b")).toBe(true);
    // tail=8 keeps the final hex characters
    expect(result.endsWith("bbd17f")).toBe(true);
  });

  it("respects custom head/tail lengths", () => {
    const result = truncateMiddle("abcdefghijklmnop", 3, 3);
    expect(result).toBe("abc…nop");
  });

  it("does not truncate when value fits within head + tail + 1", () => {
    expect(truncateMiddle("abcdefghij", 5, 4)).toBe("abcdefghij");
  });
});

describe("isHashLike", () => {
  it("detects sha256 digests", () => {
    expect(isHashLike("sha256:1090bc3a8ccfb0b55f78a494d76f8d603434f7e4553543d6e807bc7bd6bbd17f"))
      .toBe(true);
  });

  it("detects bare hex hashes of 40+ chars", () => {
    expect(isHashLike("1090bc3a8ccfb0b55f78a494d76f8d603434f7e4553")).toBe(true);
  });

  it("rejects ordinary version strings", () => {
    expect(isHashLike("latest")).toBe(false);
    expect(isHashLike("1.2.3")).toBe(false);
  });

  it("rejects short hex strings", () => {
    expect(isHashLike("abc123")).toBe(false);
  });

  it("rejects empty input", () => {
    expect(isHashLike("")).toBe(false);
  });
});
