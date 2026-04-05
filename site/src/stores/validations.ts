import { defineStore } from "pinia";
import { type Ref, ref } from "vue";
import http from "@/http";
export const useValidationStore = defineStore("validationStore", () => {
  const usernames: Ref<Record<string, boolean>> = ref({});
  const emails: Ref<Record<string, boolean>> = ref({});
  async function userAvailableRequest(
    type: "Username" | "Email",
    value: string,
  ): Promise<boolean | undefined> {
    let isAvailable: boolean | undefined = undefined;
    await http
      .post(`/api/user-management/is-taken`, {
        type: type,
        value: value,
      })
      .then(() => {
        isAvailable = true;
      })
      .catch((response) => {
        if (response.response?.status === 409) {
          isAvailable = false;
        } else if (response.response?.status === 403 || response.response?.status === 401) {
          // User doesn't have permission to check - assume available and let create operation handle it
          console.warn(`Permission denied checking ${type} availability, assuming available`);
          isAvailable = true;
        } else {
          // Other errors - log and assume available to not block form
          console.warn(`Error checking ${type} availability:`, response);
          isAvailable = true;
        }
      });
    console.log(`${type} ${value} is available: ${isAvailable}`);
    return isAvailable;
  }
  async function isUsernameInUse(username: string): Promise<boolean | undefined> {
    const lowercaseUsername = username.toLowerCase();
    if (usernames.value[lowercaseUsername] !== undefined) {
      return usernames.value[lowercaseUsername];
    }
    const isAvailable = await userAvailableRequest("Username", username);
    if (isAvailable !== undefined) {
      usernames.value[lowercaseUsername] = isAvailable;
      return isAvailable;
    } else {
      return undefined;
    }
  }
  /**
   *
   * @param email email to validate
   * @returns returns true if the email is in use, false if it is not in use, and undefined if value is not valid
   */
  async function isEmailInUse(email: string): Promise<boolean | undefined> {
    const lowercaseEmail = email.toLowerCase();
    if (emails.value[lowercaseEmail] !== undefined) {
      return emails.value[lowercaseEmail];
    }
    const isAvailable = await userAvailableRequest("Email", email);
    if (isAvailable !== undefined) {
      emails.value[lowercaseEmail] = isAvailable;
      return isAvailable;
    } else {
      return undefined;
    }
  }

  return {
    isUsernameInUse,
    isEmailInUse,
    usernames,
    emails,
  };
});
