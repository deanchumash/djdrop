import { invoke, Channel } from '@tauri-apps/api/core';

// 1x1 transparent PNG - drag icon required by the plugin
const EMPTY_ICON = 'data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==';

export interface DragOptions {
  items: string[];
  icon?: string;
}

export async function startDrag({ items, icon }: DragOptions): Promise<void> {
  const onEvent = new Channel();
  await invoke('plugin:drag|start_drag', {
    item: items,
    image: icon ?? EMPTY_ICON,
    options: { mode: 'copy' },
    onEvent,
  });
}
