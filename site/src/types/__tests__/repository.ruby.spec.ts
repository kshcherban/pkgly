import { configTypes, repositoryTypes } from "@/types/repository";

describe("Ruby repository metadata", () => {
  it("includes ruby in repositoryTypes", () => {
    const ruby = repositoryTypes.find((repo) => repo.name === "ruby");
    expect(ruby).toBeDefined();
    expect(ruby?.properName).toMatch(/ruby/i);
  });

  it("includes ruby config type", () => {
    const config = configTypes.find((entry) => entry.name === "ruby");
    expect(config).toBeDefined();
    expect(config?.title).toMatch(/Ruby/i);
  });
});

