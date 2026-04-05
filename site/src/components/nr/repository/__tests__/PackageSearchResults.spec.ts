import { mount } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import PackageSearchResults, { type PackageResult } from "@/components/nr/repository/PackageSearchResults.vue";

vi.mock("@/utils/repositorySearch", () => ({
  formatBytes: (value: number) => `${value} bytes`,
}));

const baseResult: PackageResult = {
  repositoryId: "repo-1",
  repositoryName: "Alpha",
  storageName: "Primary",
  repositoryType: "docker",
  fileName: "simple",
  cachePath: "short/path",
  size: 1024,
  modified: "2025-11-13T00:00:00Z",
};

describe("PackageSearchResults.vue", () => {
  let originalCreateRange: typeof document.createRange;
  let originalGetSelection: typeof window.getSelection;
  let createRangeSpy: ReturnType<typeof vi.fn>;
  let getSelectionSpy: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    originalCreateRange = document.createRange;
    originalGetSelection = window.getSelection;

    createRangeSpy = vi.fn(() => ({
      selectNodeContents: vi.fn(),
      collapse: vi.fn(),
      setStart: vi.fn(),
      setEnd: vi.fn(),
      getBoundingClientRect: vi.fn(),
      getClientRects: vi.fn(),
    }) as unknown as Range);
    document.createRange = createRangeSpy as unknown as typeof document.createRange;

    getSelectionSpy = vi.fn(() => ({
      removeAllRanges: vi.fn(),
      addRange: vi.fn(),
      toString: vi.fn(() => ""),
    })) as unknown as typeof window.getSelection;
    window.getSelection = getSelectionSpy as unknown as typeof window.getSelection;
  });

  afterEach(() => {
    document.createRange = originalCreateRange;
    window.getSelection = originalGetSelection;
    vi.restoreAllMocks();
  });

  it("decorates long package strings so they can wrap without layout breakage", () => {
    const longName = "test/docker/nginx:sha256:97a145fb5809fd90bebdf6671169b97e92ea99da5403c20310dcc425974a14f9";
    const longPath = "v2/test/docker/nginx/manifests/sha256:97a145fb5809fd90bebdf6671169b97e92ea99da5403c20310dcc425974a14f9";

    const wrapper = mount(PackageSearchResults, {
      props: {
        loading: false,
        error: null,
        results: [
          {
            ...baseResult,
            fileName: longName,
            cachePath: longPath,
          },
        ],
      },
    });

    const name = wrapper.get('[data-testid="package-result-name"]');
    expect(name.attributes("title")).toBe(longName);
    expect(name.classes()).toContain("package-results__wrap");

    const path = wrapper.get('[data-testid="package-result-path"]');
    expect(path.attributes("title")).toBe(longPath);
    expect(path.classes()).toContain("package-results__wrap");
  });

  it("does not emit open event when a row is clicked", async () => {
    const wrapper = mount(PackageSearchResults, {
      props: {
        loading: false,
        error: null,
        results: [baseResult],
      },
    });

    await wrapper.get("tbody tr").trigger("click");
    expect(wrapper.emitted("open")).toBeUndefined();
  });

  it("selects cell contents on click", async () => {
    const wrapper = mount(PackageSearchResults, {
      attachTo: document.body,
      props: {
        loading: false,
        error: null,
        results: [baseResult],
      },
    });

    const cell = wrapper.get('[data-testid="package-result-name"]');
    await cell.trigger("click");

    expect(getSelectionSpy).toHaveBeenCalled();
    expect(createRangeSpy).toHaveBeenCalled();
  });
});
