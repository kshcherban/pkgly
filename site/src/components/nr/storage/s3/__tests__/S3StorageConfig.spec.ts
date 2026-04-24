import { flushPromises, mount } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { defineComponent } from "vue";
import S3StorageConfig from "@/components/nr/storage/s3/S3StorageConfig.vue";
import http from "@/http";
import type { S3StorageSettings } from "@/components/nr/storage/storageTypes";

vi.mock("@/http", () => ({
  default: {
    get: vi.fn(),
  },
}));

const baseSettings = (): S3StorageSettings => ({
  bucket_name: "",
  region: undefined,
  custom_region: undefined,
  endpoint: undefined,
  credentials: {
    access_key: "",
    secret_key: "",
    session_token: "",
    role_arn: "",
    role_session_name: "",
    external_id: "",
  },
  path_style: true,
  cache: {
    enabled: false,
    path: "",
    max_bytes: 536870912,
    max_entries: 2048,
  },
});

const stubs = {
  TextInput: defineComponent({
    props: ["modelValue", "id"],
    emits: ["update:modelValue"],
    template: "<label><slot /><input :id='id' :value='modelValue' /></label>",
  }),
  SwitchInput: defineComponent({
    props: ["modelValue", "id"],
    emits: ["update:modelValue"],
    template: "<label><slot /><input :id='id' type='checkbox' :checked='modelValue' /></label>",
  }),
  TwoByFormBox: defineComponent({
    template: "<div><slot /></div>",
  }),
  DropDown: defineComponent({
    props: ["modelValue", "options", "id"],
    emits: ["update:modelValue"],
    template: "<select :id='id' :value='modelValue'><option v-for='option in options' :key='option.value' :value='option.value'>{{ option.label }}</option></select>",
  }),
  "v-autocomplete": defineComponent({
    props: ["modelValue", "items", "itemTitle", "itemValue", "id", "label"],
    emits: ["update:modelValue"],
    template: `
      <label>
        {{ label }}
        <input
          :id="id"
          data-testid="region-search"
          type="search"
          :value="modelValue" />
        <select data-testid="region-options" :value="modelValue">
          <option
            v-for="item in items"
            :key="item[itemValue]"
            :value="item[itemValue]">
            {{ item[itemTitle] }}
          </option>
        </select>
      </label>
    `,
  }),
};

describe("S3StorageConfig.vue", () => {
  beforeEach(() => {
    (http.get as vi.Mock).mockReset();
  });

  it("displays AWS region ids while preserving backend enum values", async () => {
    (http.get as vi.Mock).mockResolvedValue({
      data: ["UsEast1", "EuWest1", "SaEast1"],
    });

    const wrapper = mount(S3StorageConfig, {
      props: {
        modelValue: baseSettings(),
      },
      global: {
        stubs,
      },
    });

    await flushPromises();

    const options = wrapper.find('[data-testid="region-options"]');
    expect(options.text()).toContain("us-east-1");
    expect(options.text()).toContain("eu-west-1");
    expect(options.text()).toContain("sa-east-1");
    expect(options.text()).not.toContain("Us East1");
    expect((options.find("option").element as HTMLOptionElement).value).toBe("UsEast1");
  });

  it("uses a searchable AWS region control", async () => {
    (http.get as vi.Mock).mockResolvedValue({
      data: ["UsEast1", "EuWest1"],
    });

    const wrapper = mount(S3StorageConfig, {
      props: {
        modelValue: baseSettings(),
      },
      global: {
        stubs,
      },
    });

    await flushPromises();

    expect(wrapper.get('[data-testid="region-search"]').attributes("type")).toBe("search");
  });
});
