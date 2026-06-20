// ABOUTME: Tests MonoValue — truncated display of long hashes with copy-to-clipboard.
// ABOUTME: Covers both supported and unsupported Clipboard API environments.
import { mount } from "@vue/test-utils";
import { describe, expect, it, vi, beforeEach } from "vitest";

const copyMock = vi.fn().mockResolvedValue(undefined);
const successMock = vi.fn();

vi.mock("@/stores/alerts", () => ({
  useAlertsStore: () => ({ success: successMock, push: vi.fn(), error: vi.fn() }),
}));

import MonoValue from "@/components/ui/MonoValue.vue";
import { flushPromises } from "@vue/test-utils";

function mountMonoValue(value: string) {
  return mount(MonoValue, {
    props: { value },
    global: { stubs: { "v-icon": true } },
  });
}

describe("MonoValue", () => {
  beforeEach(() => {
    copyMock.mockClear();
    successMock.mockClear();
    Object.assign(navigator, { clipboard: { writeText: copyMock } });
  });

  it("renders short values verbatim", () => {
    const wrapper = mountMonoValue("latest");
    expect(wrapper.text()).toContain("latest");
    expect(wrapper.text()).not.toContain("…");
  });

  it("truncates long hashes and exposes the full value via title", () => {
    const digest = "sha256:1090bc3a8ccfb0b55f78a494d76f8d603434f7e4553543d6e807bc7bd6bbd17f";
    const wrapper = mountMonoValue(digest);

    expect(wrapper.text()).toContain("…");
    expect(wrapper.text()).not.toContain(digest);
    const code = wrapper.get("code");
    expect(code.attributes("title")).toBe(digest);
  });

  it("copies the full value to the clipboard and confirms via alert", async () => {
    const digest = "sha256:1090bc3a8ccfb0b55f78a494d76f8d603434f7e4553543d6e807bc7bd6bbd17f";
    const wrapper = mountMonoValue(digest);

    await wrapper.get('[data-testid="mono-copy"]').trigger("click");
    await flushPromises();

    expect(copyMock).toHaveBeenCalledWith(digest);
    expect(successMock).toHaveBeenCalled();
  });

  it("hides the copy action when clipboard api is unavailable", () => {
    Object.assign(navigator, { clipboard: undefined });

    const wrapper = mountMonoValue("latest");

    expect(wrapper.find('[data-testid="mono-copy"]').exists()).toBe(false);
  });
});
