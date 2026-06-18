import { createApp } from "vue";
import { createPinia } from "pinia";
import App from "./App.vue";
import router from "./router";
import { patchWindowControls } from "./headless-patch";
import "./assets/index.css";

async function bootstrap() {
  await patchWindowControls();

  const app = createApp(App);
  app.use(createPinia());
  app.use(router);
  app.mount("#app");
}

bootstrap();
