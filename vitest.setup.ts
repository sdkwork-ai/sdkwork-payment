import "@testing-library/jest-dom/vitest";

// Radix UI components (Select, ScrollArea, Tooltip, …) rely on ResizeObserver,
// which jsdom does not implement. A no-op polyfill keeps component tests that
// render these controls working.
if (typeof globalThis.ResizeObserver === "undefined") {
  class ResizeObserverStub {
    observe(): void {}
    unobserve(): void {}
    disconnect(): void {}
  }
  globalThis.ResizeObserver = ResizeObserverStub as unknown as typeof ResizeObserver;
}

// Radix Select scrolls the highlighted option into view on open; jsdom does
// not implement scrollIntoView, so provide a no-op.
if (typeof Element !== "undefined" && typeof Element.prototype.scrollIntoView !== "function") {
  Element.prototype.scrollIntoView = () => undefined;
}
