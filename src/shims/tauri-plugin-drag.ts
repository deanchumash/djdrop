// Runtime shim for @tauri-apps/plugin-drag
// In production Tauri builds the real plugin is injected by Rust.
// In tests, Vitest mocks this entire module via src/test/setup.ts.

export interface DragItem {
  items: string[];
  icon?: string;
}

export async function startDrag(_item: DragItem): Promise<void> {
  // no-op outside Tauri — runtime replaces this via plugin
}
