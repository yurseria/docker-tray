import { useEffect, useRef } from "react";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { PhysicalSize } from "@tauri-apps/api/dpi";

const BASE_WIDTH = 420;
const MIN_HEIGHT = 300;
const MAX_HEIGHT = 900;

function getUiScale(): number {
  const fs = parseFloat(getComputedStyle(document.documentElement).fontSize);
  return fs / 13; // 13px is the base font size
}

export function useAutoResize(ref: React.RefObject<HTMLElement | null>) {
  const lastHeight = useRef(0);
  const lastWidth = useRef(0);

  useEffect(() => {
    const el = ref.current;
    if (!el) return;

    const update = async () => {
      const uiScale = getUiScale();
      const width = Math.round(BASE_WIDTH * uiScale);

      // Sum each direct child's scrollHeight to get the "natural" total
      // This bypasses flex constraints and overflow on .app/.content
      let natural = 0;
      for (const child of el.children) {
        natural += child.scrollHeight;
      }

      const clamped = Math.max(MIN_HEIGHT, Math.min(MAX_HEIGHT, natural + 10));

      const heightChanged = Math.abs(clamped - lastHeight.current) > 2;
      const widthChanged = width !== lastWidth.current;
      if (!heightChanged && !widthChanged) return;
      lastHeight.current = clamped;
      lastWidth.current = width;

      try {
        const win = getCurrentWebviewWindow();
        const scale = await win.scaleFactor();
        await win.setSize(new PhysicalSize(
          Math.round(width * scale),
          Math.round(clamped * scale),
        ));
      } catch {
        // ignore — window may not be ready
      }
    };

    // ResizeObserver catches layout shifts (expand/collapse)
    const observer = new ResizeObserver(() => update());
    observer.observe(el);

    // MutationObserver catches DOM changes (tab switch, data fetch, settings toggle)
    const mutObserver = new MutationObserver(() => update());
    mutObserver.observe(el, { childList: true, subtree: true });

    update();

    return () => {
      observer.disconnect();
      mutObserver.disconnect();
    };
  }, [ref]);
}
