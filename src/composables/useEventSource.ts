import { ref, onUnmounted } from "vue";
import { listen, type UnlistenFn } from "@/lib/api";

export function useEventSource<T>(eventName: string) {
  const data = ref<T | null>(null);
  const error = ref<string | null>(null);
  let unlisten: UnlistenFn | null = null;

  async function start() {
    try {
      unlisten = await listen<T>(eventName, (event) => {
        data.value = event.payload;
      });
    } catch (e) {
      error.value = String(e);
    }
  }

  onUnmounted(() => {
    if (unlisten) unlisten();
  });

  start();
  return { data, error };
}
