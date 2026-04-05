import { defineStore } from "pinia";

export type AlertKind = "success" | "info" | "warning" | "error";

export interface AlertMessage {
  id: number;
  kind: AlertKind;
  title: string;
  message?: string;
}

const DEFAULT_DISMISS_MS = 6000;
const SUCCESS_DISMISS_MS = 4000;
const PERSISTENT_DISMISS_MS = 0;

function defaultDismissMs(kind: AlertKind): number {
  switch (kind) {
    case "success":
      return SUCCESS_DISMISS_MS;
    case "info":
      return DEFAULT_DISMISS_MS;
    case "warning":
    case "error":
      return PERSISTENT_DISMISS_MS;
  }
}

export const useAlertsStore = defineStore("alerts", {
  state: () => ({
    alerts: [] as AlertMessage[],
    counter: 0,
  }),
  actions: {
    push(kind: AlertKind, title: string, message?: string, dismissAfterMs = defaultDismissMs(kind)) {
      const id = ++this.counter;
      this.alerts.push({ id, kind, title, message });

      if (dismissAfterMs > 0) {
        window.setTimeout(() => {
          this.dismiss(id);
        }, dismissAfterMs);
      }

      return id;
    },
    success(title: string, message?: string, dismissAfterMs = defaultDismissMs("success")) {
      return this.push("success", title, message, dismissAfterMs);
    },
    info(title: string, message?: string, dismissAfterMs = defaultDismissMs("info")) {
      return this.push("info", title, message, dismissAfterMs);
    },
    warning(title: string, message?: string, dismissAfterMs = defaultDismissMs("warning")) {
      return this.push("warning", title, message, dismissAfterMs);
    },
    error(title: string, message?: string, dismissAfterMs = defaultDismissMs("error")) {
      return this.push("error", title, message, dismissAfterMs);
    },
    dismiss(id: number) {
      this.alerts = this.alerts.filter((alert) => alert.id !== id);
    },
    clear() {
      this.alerts = [];
    },
  },
});
