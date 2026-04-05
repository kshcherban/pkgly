import "./assets/styles/main.scss";
import "vue-final-modal/style.css";

import { createApp } from "vue";
import { createPinia, setActivePinia } from "pinia";
import { createVfm } from "vue-final-modal";
import App from "./App.vue";
import router from "./router";
import { createMetaManager } from "vue-meta";
import { FontAwesomeIcon } from "@fortawesome/vue-fontawesome";
import { library } from "@fortawesome/fontawesome-svg-core";
import {
  faCalendar,
  faFileImage,
  faFileText,
  faGear,
  faUser,
  faBars,
  faX,
  faRightToBracket,
  faPenToSquare,
  faFloppyDisk,
  faArrowLeft,
  faHome,
  faArrowRight,
  faAnglesRight,
  faAnglesLeft,
  faEye,
  faEyeSlash,
  faUsers,
  faBoxOpen,
  faBoxesPacking,
  faToolbox,
  faUserPlus,
  faAngleDown,
  faCircleXmark,
  faCheckCircle,
  faFile,
  faFolder,
} from "@fortawesome/free-solid-svg-icons";

import { sessionStore } from "./stores/session";
import { autoAnimatePlugin } from "@formkit/auto-animate/vue";
import { applyThemeTokens } from "@/utils/themeTokens";
import vuetify from "./plugins/vuetify";

const pinia = createPinia();
setActivePinia(pinia);
const app = createApp(App);
const vfm = createVfm();
applyThemeTokens();

router.beforeEach(async (to) => {
  const store = sessionStore(pinia);
  const requiresIdentity =
    to.meta.requiresAuth === true || to.meta.requiresIdentity === true;
  if (!requiresIdentity) {
    return true;
  }
  if (store.session === undefined) {
    await store.updateUser();
  }
  if (store.session === undefined) {
    return {
      name: "login",
      query: { redirect: to.fullPath },
    };
  }
  return true;
});

app.use(router);

/* add icons to the library */
library.add(faGear);
library.add(faUser);
library.add(faBars);
library.add(faFileText);
library.add(faFileImage);
library.add(faCalendar);
library.add(faRightToBracket);
library.add(faX);
library.add(faPenToSquare);
library.add(faFloppyDisk);
library.add(faArrowLeft);
library.add(faHome);
library.add(faArrowRight);
library.add(faAnglesRight);
library.add(faAnglesLeft);
library.add(faAngleDown);
library.add(faEye);
library.add(faEyeSlash);
library.add(faUsers);
library.add(faBoxOpen);
library.add(faBoxesPacking);
library.add(faToolbox);
library.add(faUserPlus);
library.add(faCircleXmark);
library.add(faCheckCircle);
library.add(faFile);
library.add(faFolder);
app.use(createMetaManager());
app.use(pinia);
app.use(vuetify);
app.component("font-awesome-icon", FontAwesomeIcon);
app.use(autoAnimatePlugin);
app.use(vfm);
app.mount("#app");
