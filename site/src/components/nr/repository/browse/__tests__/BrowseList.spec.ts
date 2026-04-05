import { flushPromises, mount } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import BrowseList from "@/components/nr/repository/browse/BrowseList.vue";

const repository = {
  id: "repo-1",
  storage_name: "local",
  storage_id: "storage-1",
  name: "dockerhub",
  repository_type: "docker",
  repository_kind: "hosted",
  active: true,
  visibility: "Public",
  updated_at: "2026-03-26T10:00:00Z",
  created_at: "2026-03-25T10:00:00Z",
  auth_enabled: false,
  storage_usage_bytes: null,
  storage_usage_updated_at: null,
} as const;

describe("BrowseList.vue", () => {
  const originalCreateElement = document.createElement.bind(document);
  let createdLinks: HTMLAnchorElement[] = [];
  let openSpy: ReturnType<typeof vi.spyOn>;

  beforeEach(() => {
    createdLinks = [];
    openSpy = vi.spyOn(window, "open").mockImplementation(() => null);
    vi.spyOn(document, "createElement").mockImplementation(((tagName: string) => {
      const element = originalCreateElement(tagName);
      if (tagName.toLowerCase() === "a") {
        const anchor = element as HTMLAnchorElement;
        anchor.click = vi.fn();
        createdLinks.push(anchor);
      }
      return element;
    }) as typeof document.createElement);
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("downloads only the selected files", async () => {
    const wrapper = mount(BrowseList, {
      props: {
        files: [
          {
            type: "File",
            value: {
              name: "18",
              mime_type: "application/json",
              file_size: 42,
              modified: "2026-03-26T10:00:00Z",
              created: "2026-03-26T10:00:00Z",
            },
          },
          {
            type: "File",
            value: {
              name: "sha256:abcd",
              mime_type: "application/json",
              file_size: 99,
              modified: "2026-03-26T10:00:00Z",
              created: "2026-03-26T10:00:00Z",
            },
          },
        ],
        totalFiles: 2,
        currentPath: "local/dockerhub/postgres",
        repository,
      },
      global: {
        stubs: {
          "font-awesome-icon": true,
        },
      },
    });

    await wrapper.get('[data-testid="browse-select-file-18"]').setValue(true);
    await wrapper.get('[data-testid="browse-download-selected"]').trigger("click");
    await flushPromises();

    expect(createdLinks).toHaveLength(1);
    expect(createdLinks[0].href).toContain(
      "/repositories/local/dockerhub/v2/postgres/manifests/18",
    );
    expect(createdLinks[0].download).toBe("18");
    expect(createdLinks[0].click).toHaveBeenCalledTimes(1);
  });

  it("opens downloadable Docker files from the translated manifest route", async () => {
    const wrapper = mount(BrowseList, {
      props: {
        files: [
          {
            type: "File",
            value: {
              name: "13",
              mime_type: "application/json",
              file_size: 42,
              modified: "2026-03-26T10:00:00Z",
              created: "2026-03-26T10:00:00Z",
            },
          },
        ],
        totalFiles: 1,
        currentPath: "local/dockerhub/debian",
        repository,
      },
      global: {
        stubs: {
          "font-awesome-icon": true,
        },
      },
    });

    await wrapper.get('[data-testid="browse-row-file-13"]').trigger("click");

    expect(openSpy).toHaveBeenCalledWith(
      "http://localhost:3000/repositories/local/dockerhub/v2/debian/manifests/13",
      "_blank",
    );
  });

  it("selects all downloadable files and excludes directories and Docker tag metadata", async () => {
    const wrapper = mount(BrowseList, {
      props: {
        files: [
          {
            type: "Directory",
            value: {
              name: "manifests",
              number_of_files: 2,
            },
          },
          {
            type: "File",
            value: {
              name: "18",
              mime_type: "application/json",
              file_size: 42,
              modified: "2026-03-26T10:00:00Z",
              created: "2026-03-26T10:00:00Z",
            },
          },
          {
            type: "File",
            value: {
              name: "18.nr-docker-tagmeta",
              mime_type: "application/json",
              file_size: 1,
              modified: "2026-03-26T10:00:00Z",
              created: "2026-03-26T10:00:00Z",
            },
          },
          {
            type: "File",
            value: {
              name: "sha256:abcd",
              mime_type: "application/json",
              file_size: 99,
              modified: "2026-03-26T10:00:00Z",
              created: "2026-03-26T10:00:00Z",
            },
          },
        ],
        totalFiles: 4,
        currentPath: "local/dockerhub/postgres",
        repository,
      },
      global: {
        stubs: {
          "font-awesome-icon": true,
        },
      },
    });

    await wrapper.get('[data-testid="browse-select-all-files"]').setValue(true);
    await wrapper.get('[data-testid="browse-download-selected"]').trigger("click");
    await flushPromises();

    expect(createdLinks).toHaveLength(2);
    expect(createdLinks.map((link) => link.download)).toEqual(["18", "sha256:abcd"]);
    expect(createdLinks.map((link) => link.href)).toEqual([
      "http://localhost:3000/repositories/local/dockerhub/v2/postgres/manifests/18",
      "http://localhost:3000/repositories/local/dockerhub/v2/postgres/manifests/sha256:abcd",
    ]);
  });
});
