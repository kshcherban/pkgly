import { afterEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";

describe("alerts store", () => {
  afterEach(() => {
    vi.resetModules();
    vi.useRealTimers();
  });

  async function createStore() {
    vi.useFakeTimers();
    vi.doUnmock("@/stores/alerts");
    const { useAlertsStore } = await import("@/stores/alerts");
    setActivePinia(createPinia());
    return useAlertsStore();
  }

  it("auto-dismisses success alerts after the success timeout", async () => {
    const store = await createStore();

    store.success("Saved", "Changes applied.");

    expect(store.$state.alerts).toHaveLength(1);

    vi.advanceTimersByTime(3_999);
    expect(store.$state.alerts).toHaveLength(1);

    vi.advanceTimersByTime(1);
    expect(store.$state.alerts).toHaveLength(0);
  });

  it("keeps error alerts visible until dismissed", async () => {
    const store = await createStore();

    store.error("Failed", "Review the form and try again.");

    vi.advanceTimersByTime(30_000);
    expect(store.$state.alerts).toHaveLength(1);

    store.dismiss(store.$state.alerts[0]!.id);
    expect(store.$state.alerts).toHaveLength(0);
  });

  it("supports informational and warning alerts", async () => {
    const store = await createStore();

    store.info("Heads up", "Refresh may take a minute.");
    store.warning("Feature unavailable", "Repository activation is coming soon.");

    expect(store.$state.alerts).toEqual([
      expect.objectContaining({ kind: "info", title: "Heads up" }),
      expect.objectContaining({ kind: "warning", title: "Feature unavailable" }),
    ]);

    vi.advanceTimersByTime(6_000);

    expect(store.$state.alerts).toEqual([
      expect.objectContaining({ kind: "warning", title: "Feature unavailable" }),
    ]);
  });
});
