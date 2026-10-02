import { useEffect, useRef, type RefObject } from "react";

/**
 * Drive a CSS `scroll-fade` edge mask from scroll position. Returns a ref to
 * bind to the scrolling container. On scroll (rAF-throttled), on attach, and on
 * window resize it writes `data-fade-top` / `data-fade-bottom` ("true"/"false")
 * on that container so CSS can mask an edge only when there is content beyond
 * it:
 *
 *   data-fade-top="false"    when scrollTop <= 0                          (top reached)
 *   data-fade-bottom="false" when scrollTop + clientHeight >= scrollHeight - 1 (bottom reached)
 *
 * The ref'd node may mount after this effect first runs — a screen that shows
 * a loading placeholder before its scroller (e.g. Settings). We poll on rAF
 * until the node is present, then attach, so the fade still activates.
 */
export function useScrollFade<T extends HTMLElement>(): RefObject<T> {
  const ref = useRef<T>(null!);

  useEffect(() => {
    let scrollRaf = 0;
    let attachRaf = 0;
    let attempts = 0;
    let cleanedUp = false;
    let attached: HTMLElement | null = null;

    const update = (): void => {
      scrollRaf = 0;
      const el = ref.current;
      if (!el) return;
      el.dataset.fadeTop = el.scrollTop <= 0 ? "false" : "true";
      el.dataset.fadeBottom =
        el.scrollTop + el.clientHeight >= el.scrollHeight - 1 ? "false" : "true";
    };

    const scheduleScroll = (): void => {
      if (scrollRaf) return;
      scrollRaf = requestAnimationFrame(update);
    };

    const attach = (el: HTMLElement): void => {
      attached = el;
      update();
      el.addEventListener("scroll", scheduleScroll, { passive: true });
      window.addEventListener("resize", scheduleScroll);
    };

    const tryAttach = (): void => {
      if (cleanedUp) return;
      const el = ref.current;
      if (el) {
        attach(el);
        return;
      }
      // Bail after ~5s so a ref that never binds can't spin forever.
      if (attempts++ < 300) {
        attachRaf = requestAnimationFrame(tryAttach);
      }
    };

    tryAttach();

    return () => {
      cleanedUp = true;
      if (scrollRaf) cancelAnimationFrame(scrollRaf);
      if (attachRaf) cancelAnimationFrame(attachRaf);
      if (attached) {
        attached.removeEventListener("scroll", scheduleScroll);
        window.removeEventListener("resize", scheduleScroll);
      }
    };
  }, []);

  return ref;
}