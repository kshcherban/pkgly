import { defineStore } from "pinia";
import type { Me, Session, UserResponseType } from "@/types/base";
import http from "@/http";

interface SessionState {
  session: Session | undefined;
  user: UserResponseType | undefined;
}

export const sessionStore = defineStore("sessionStore", {
  state: (): SessionState => ({
    session: undefined,
    user: undefined,
  }),
  getters: {
    isAdmin: (state) => Boolean(state.user?.admin),
  },
  actions: {
    login(me: Me) {
      this.user = me.user;
      this.session = me.session;
    },
    async logout() {
      try {
        await http.post("/api/user/logout");
      } catch (error) {
        console.error("Failed to logout cleanly", error);
      } finally {
        this.session = undefined;
        this.user = undefined;
      }
    },
    async updateUser(): Promise<UserResponseType | undefined> {
      try {
        const response = await http.get<Me>("/api/user/me");
        const fetchedSession = response.data.session;
        this.session = {
          ...fetchedSession,
          expires: new Date(fetchedSession.expires),
          created: new Date(fetchedSession.created),
        };
        this.user = response.data.user;
        return response.data.user;
      } catch (error) {
        console.error("Failed to refresh user information", error);
        this.session = undefined;
        this.user = undefined;
        return undefined;
      }
    },
  },
});
