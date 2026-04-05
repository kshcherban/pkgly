import { flushPromises, mount } from "@vue/test-utils";
import { nextTick, ref } from "vue";
import { describe, expect, it, vi } from "vitest";

vi.mock("@/http", () => ({
  default: {
    get: vi.fn(),
    put: vi.fn(),
  },
}));

import PhpConfig from "../PhpConfig.vue";
import http from "@/http";

const dropDownStub = {
  template: `<label class="stub-dropdown">
    <slot />
    <select :value="modelValue" @change="$emit('update:modelValue', $event.target.value)">
      <option v-for="opt in options" :key="opt.value" :value="opt.value">{{ opt.label }}</option>
    </select>
  </label>`,
  props: ["modelValue", "options", "disabled"],
};

const textInputStub = {
  template: `<label class="stub-text-input">
    <slot />
    <input :value="modelValue" @input="$emit('update:modelValue', $event.target.value)" />
  </label>`,
  props: {
    modelValue: {
      type: [String, Number],
      default: "",
    },
  },
};

describe("PhpConfig.vue", () => {
  it("defaults proxy routes to Packagist when switching to proxy", async () => {
    const model = ref({ type: "Hosted" });
    let wrapper: any;
    const update = vi.fn((val) => {
      model.value = val;
      if (wrapper) {
        wrapper.setProps({ modelValue: val });
      }
    });
    wrapper = mount(PhpConfig, {
      props: {
        modelValue: model.value,
        "onUpdate:modelValue": update,
      },
      global: {
        stubs: {
          DropDown: dropDownStub,
          TextInput: textInputStub,
          ProxyCacheNotice: { template: "<div class='proxy-notice' />" },
          SubmitButton: { template: "<button><slot /></button>" },
          "v-btn": { template: "<button><slot /></button>" },
          "v-alert": { template: "<div><slot /></div>" },
        },
      },
    });

    await wrapper.find("select").setValue("Proxy");
    await flushPromises();

    const latest = update.mock.calls.at(-1)?.[0];
    expect(latest?.type).toBe("Proxy");
    expect(latest?.config.routes[0].url).toBe("https://repo.packagist.org");
    expect(latest?.config.routes[0].name).toBe("Packagist");
  });

  it("loads existing configuration when repository is provided", async () => {
    (http.get as vi.Mock).mockResolvedValueOnce({
      data: { type: "Proxy", config: { routes: [{ url: "https://mirror.example", name: "Mirror" }] } },
    });

    const model = ref({ type: "Hosted" });
    let wrapper: any;
    const update = vi.fn((val) => {
      model.value = val;
      if (wrapper) {
        wrapper.setProps({ modelValue: val });
      }
    });
    wrapper = mount(PhpConfig, {
      props: {
        repository: "repo-1",
        modelValue: model.value,
        "onUpdate:modelValue": update,
      },
      global: {
        stubs: {
          DropDown: dropDownStub,
          TextInput: textInputStub,
          ProxyCacheNotice: { template: "<div class='proxy-notice' />" },
          SubmitButton: { template: "<button><slot /></button>" },
          "v-btn": { template: "<button><slot /></button>" },
          "v-alert": { template: "<div><slot /></div>" },
        },
      },
    });

    await flushPromises();
    await nextTick();
    expect(http.get).toHaveBeenCalledWith("/api/repository/repo-1/config/php");

    const proxyCall = update.mock.calls.find((call) => call?.[0]?.type === "Proxy");
    expect(proxyCall?.[0]?.config.routes[0].url).toBe("https://mirror.example");
  });
});
