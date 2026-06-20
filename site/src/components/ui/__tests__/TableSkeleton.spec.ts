// ABOUTME: Tests TableSkeleton renders an accessible loading placeholder with N rows.
import { mount } from "@vue/test-utils";
import { describe, expect, it } from "vitest";
import { defineComponent } from "vue";
import TableSkeleton from "@/components/ui/TableSkeleton.vue";

const VSkeletonLoader = defineComponent({
  name: "VSkeletonLoader",
  template: "<div class='v-skeleton-loader'><slot /></div>",
});

const wrapper = (props: Record<string, unknown>) =>
  mount(TableSkeleton, {
    props,
    global: { stubs: { "v-skeleton-loader": VSkeletonLoader } },
  });

describe("TableSkeleton", () => {
  it("renders the requested number of skeleton rows", () => {
    const w = wrapper({ rows: 4 });
    expect(w.findAll(".v-skeleton-loader")).toHaveLength(4);
  });

  it("defaults to five rows", () => {
    const w = wrapper({});
    expect(w.findAll(".v-skeleton-loader")).toHaveLength(5);
  });

  it("exposes an accessible loading status", () => {
    const w = wrapper({ rows: 3 });
    const root = w.find("[role='status']");
    expect(root.exists()).toBe(true);
    expect(root.attributes("aria-label")).toContain("Loading");
  });
});
