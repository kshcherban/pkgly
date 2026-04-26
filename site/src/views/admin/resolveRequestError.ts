import { isAxiosError } from "axios";

export function resolveRequestError(
  error: unknown,
  fallbackTitle: string,
  fallbackMessage: string,
  conflictTitle?: string,
  conflictMessage?: string,
): { title: string; message: string; debugMessage: string } {
  const fallback = {
    title: fallbackTitle,
    message: fallbackMessage,
    debugMessage: typeof error === "string" ? error : JSON.stringify(error),
  };

  if (isAxiosError(error)) {
    const status = error.response?.status;
    const data = error.response?.data;
    let payloadMessage: string | undefined;

    if (typeof data === "string" && data.trim().length > 0) {
      payloadMessage = data.trim();
    } else if (typeof data === "object" && data !== null && "message" in data) {
      const candidate = (data as { message?: unknown }).message;
      if (typeof candidate === "string" && candidate.trim().length > 0) {
        payloadMessage = candidate.trim();
      }
    }

    if (status === 409 && conflictTitle) {
      return {
        title: conflictTitle,
        message: conflictMessage ?? payloadMessage ?? fallbackMessage,
        debugMessage: JSON.stringify(error.toJSON?.() ?? error),
      };
    }

    if (payloadMessage) {
      return {
        title: fallbackTitle,
        message: payloadMessage,
        debugMessage: JSON.stringify(error.toJSON?.() ?? error),
      };
    }

    return {
      title: fallbackTitle,
      message: `Request failed${status ? ` with status ${status}` : ""}.`,
      debugMessage: JSON.stringify(error.toJSON?.() ?? error),
    };
  }

  if (error instanceof Error) {
    return {
      title: fallbackTitle,
      message: error.message,
      debugMessage: error.stack ?? error.message,
    };
  }

  return fallback;
}
