import { configTypes, repositoryTypes } from "@/types/repository";

describe("NuGet repository metadata", () => {
  it("includes nuget in repositoryTypes", () => {
    const nuget = repositoryTypes.find((repo) => repo.name === "nuget");
    expect(nuget).toBeDefined();
    expect(nuget?.properName).toMatch(/nuget/i);
  });

  it("includes nuget config type", () => {
    const config = configTypes.find((entry) => entry.name === "nuget");
    expect(config).toBeDefined();
    expect(config?.title).toMatch(/NuGet/i);
  });
});
