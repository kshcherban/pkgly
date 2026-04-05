import { configTypes, repositoryTypes } from "@/types/repository";

describe("Cargo repository metadata", () => {
  it("includes cargo in repositoryTypes", () => {
    const cargo = repositoryTypes.find((repo) => repo.name === "cargo");
    expect(cargo).toBeDefined();
    expect(cargo?.properName).toMatch(/cargo/i);
  });

  it("includes cargo config type", () => {
    const config = configTypes.find((entry) => entry.name === "cargo");
    expect(config).toBeDefined();
    expect(config?.title).toMatch(/Cargo/i);
  });
});
