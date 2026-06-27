import { startDrag } from '@tauri-apps/plugin-drag';
import type { DownloadItem } from '../types';

const SOURCE_ICONS: Record<string, string> = {
  youtube: '▶', soundcloud: '☁', spotify: '♫', qobuz: '◈',
  bpmsupreme: 'B', clubkillers: 'C', livedjservice: 'L',
};

interface Props { item: DownloadItem; onStart?: () => void }

export function QueueItem({ item, onStart }: Props) {
  const handleDragStart = async (e: React.DragEvent) => {
    if (!item.file_path) return;
    e.preventDefault();
    await startDrag({ items: [item.file_path] });
  };

  const statusLabel = {
    queued: '…', downloading: `${item.progress ?? 0}%`,
    analyzing: 'analyzing', done: '', error: '✕',
  }[item.status];

  return (
    <div className={`queue-item queue-item--${item.status}`} draggable={!!item.file_path} onDragStart={handleDragStart}>
      <span className="queue-item__source">{SOURCE_ICONS[item.source ?? ''] ?? '?'}</span>
      <span className="queue-item__name">{item.track_name ?? item.input}</span>
      {item.bpm ? <span className="queue-item__bpm">{item.bpm}</span> : null}
      {item.key ? <span className="queue-item__key">{item.key}</span> : null}
      {item.status === 'queued' && onStart
        ? <button className="queue-item__start" onClick={onStart}>▶</button>
        : <span className="queue-item__status">{statusLabel}</span>
      }
    </div>
  );
}
