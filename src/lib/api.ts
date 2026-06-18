/**
 * 统一 API 调用层
 *
 * 在 Tauri GUI 模式下直接使用 @tauri-apps/api 的 invoke/listen。
 * 在无头（headless）模式下，自动替换为 HTTP 调用 + SSE 事件流。
 * 所有 store 和 view 统一从此模块导入 invoke/listen。
 */

import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import { listen as tauriListen, type UnlistenFn, type Event } from "@tauri-apps/api/event";

const HEADLESS_KEY = "__FILESWEEP_HEADLESS__";

function isHeadless(): boolean {
  return !!(window as any)[HEADLESS_KEY];
}

function getBaseUrl(): string {
  return window.location.origin;
}

/** 无头模式的 invoke：通过 HTTP POST 调用后端 API */
async function headlessInvoke(cmd: string, args?: Record<string, any>): Promise<any> {
  if (cmd.startsWith("plugin:dialog|")) {
    if (cmd === "plugin:dialog|confirm" || cmd === "plugin:dialog|ask") {
      return args?.title || "ok";
    }
    return null;
  }

  const response = await fetch(`${getBaseUrl()}/api/invoke/${cmd}`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(args || {}),
  });

  const result = await response.json();
  if (!result.ok) {
    throw new Error(result.error || "Unknown error");
  }
  return result.data;
}

/**
 * SSE 事件管理器：在无头模式下，所有 listen 调用共享一个 SSE 连接。
 * 收到事件后按 event name 分发给对应的 handler。
 */
let sseConnection: EventSource | null = null;
const sseHandlers = new Map<string, Set<(event: any) => void>>();

function ensureSSE(): EventSource {
  if (sseConnection) return sseConnection;

  const es = new EventSource(`${getBaseUrl()}/api/events`);
  es.onmessage = (e) => {
    try {
      const msg = JSON.parse(e.data);
      if (msg.event && sseHandlers.has(msg.event)) {
        const handlers = sseHandlers.get(msg.event)!;
        // 构造类似 Tauri Event 的对象
        const payload = typeof msg.data === "string" ? JSON.parse(msg.data) : msg.data;
        for (const handler of handlers) {
          handler({ event: msg.event, payload, id: 0 });
        }
      }
    } catch {
      // ignore parse errors
    }
  };
  es.onerror = () => {
    // 自动重连由 EventSource 内置处理
  };

  sseConnection = es;
  return es;
}

/** 无头模式的 listen：通过 SSE 接收事件 */
async function headlessListen<T>(event: string, handler: (event: Event<T>) => void): Promise<UnlistenFn> {
  ensureSSE();

  if (!sseHandlers.has(event)) {
    sseHandlers.set(event, new Set());
  }
  sseHandlers.get(event)!.add(handler as (event: any) => void);

  // 返回取消监听函数
  return () => {
    sseHandlers.get(event)?.delete(handler as (event: any) => void);
    if (sseHandlers.get(event)?.size === 0) {
      sseHandlers.delete(event);
    }
  };
}

/**
 * 统一 invoke：自动判断环境
 */
export async function invoke<T = any>(cmd: string, args?: Record<string, any>): Promise<T> {
  if (isHeadless()) {
    return headlessInvoke(cmd, args) as T;
  }
  return tauriInvoke<T>(cmd, args);
}

/**
 * 统一 listen：自动判断环境
 */
export async function listen<T = any>(event: string, handler: (event: Event<T>) => void): Promise<UnlistenFn> {
  if (isHeadless()) {
    return headlessListen<T>(event, handler);
  }
  return tauriListen<T>(event, handler);
}

export type { UnlistenFn };
