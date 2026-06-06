// ABOUTME: Verifies session discovery behavior for authenticated and guest requests.
// ABOUTME: Distinguishes expected unauthenticated responses from unexpected failures.
import { createPinia, setActivePinia } from "pinia";
import { beforeEach, describe, expect, it, vi } from "vitest";
import http from "@/http";
import { sessionStore } from "@/stores/session";

vi.mock("@/http", () => ({
  default: {
    get: vi.fn(),
    post: vi.fn(),
  },
}));

describe("sessionStore", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    vi.clearAllMocks();
  });

  it("treats a 401 during session discovery as an expected guest state", async () => {
    const consoleError = vi.spyOn(console, "error").mockImplementation(() => {});
    vi.mocked(http.get).mockRejectedValueOnce({ response: { status: 401 } });

    await expect(sessionStore().updateUser()).resolves.toBeUndefined();

    expect(consoleError).not.toHaveBeenCalled();
    consoleError.mockRestore();
  });

  it("reports unexpected session discovery failures", async () => {
    const error = new Error("network down");
    const consoleError = vi.spyOn(console, "error").mockImplementation(() => {});
    vi.mocked(http.get).mockRejectedValueOnce(error);

    await expect(sessionStore().updateUser()).resolves.toBeUndefined();

    expect(consoleError).toHaveBeenCalledWith(
      "Failed to refresh user information",
      error,
    );
    consoleError.mockRestore();
  });
});
