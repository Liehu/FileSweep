import { invoke, listen, type UnlistenFn } from "@/lib/api";

export function useTauri() {
  async function call<T>(command: string, args?: Record<string, any>): Promise<T> {
    try {
      return await invoke<T>(command, args);
    } catch (e) {
      console.error(`Tauri command ${command} failed:`, e);
      throw e;
    }
  }

  async function on<T>(event: string, handler: (payload: T) => void): Promise<UnlistenFn> {
    return listen<T>(event, (event) => handler(event.payload));
  }

  return { call, on };
}
