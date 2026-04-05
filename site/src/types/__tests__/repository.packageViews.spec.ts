import {
  configTypes,
  repositoryTypes,
  shouldDisplayRepositoryIndexingWarning,
  supportsRepositoryPackageView,
  type FrontendRepositoryType,
} from "@/types/repository";

describe("Repository package view support", () => {
  it("supports package views for every frontend repository type", () => {
    const unsupported = repositoryTypes.filter(
      (repo: FrontendRepositoryType) => !supportsRepositoryPackageView(repo.name),
    );
    expect(unsupported).toEqual([]);
  });

  it("does not support package views for unknown repository types", () => {
    expect(supportsRepositoryPackageView("unknown")).toBe(false);
  });

  it("suppresses indexing warnings for Ruby proxy repositories", () => {
    expect(shouldDisplayRepositoryIndexingWarning("ruby", "proxy")).toBe(false);
  });

  it("keeps indexing warnings for non-Ruby proxy repositories", () => {
    expect(shouldDisplayRepositoryIndexingWarning("npm", "proxy")).toBe(true);
  });

  it("does not expose the removed repository page config", () => {
    expect(configTypes.find((configType) => configType.name === "page")).toBeUndefined();
  });
});
