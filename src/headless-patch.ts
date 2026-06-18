/**
 * 无头模式辅助模块
 *
 * 仅负责：headless 标记 + 窗口控制函数 no-op。
 * invoke/listen 的切换由 @/lib/api 统一处理，不再 patch ESM 模块。
 */

export function isHeadless(): boolean {
  return !!(window as any).__FILESWEEP_HEADLESS__;
}

/**
 * 无头模式下将窗口控制函数替换为 no-op
 */
export async function patchWindowControls() {
  if (!isHeadless()) return;

  try {
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    const currentWindow = getCurrentWindow();
    if (currentWindow) {
      (currentWindow as any).minimize = async () => {};
      (currentWindow as any).toggleMaximize = async () => {};
      (currentWindow as any).close = async () => {
        if (confirm("确定关闭 FileSweep？")) {
          window.close();
        }
      };
      (currentWindow as any).startDragging = async () => {};
    }
  } catch {
    // ignore
  }

  console.log("[FileSweep] 无头模式已激活");
}
