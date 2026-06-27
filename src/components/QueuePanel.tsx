import { QueueItem } from './QueueItem';
import type { DownloadItem } from '../types';

interface Props { items: DownloadItem[]; onClose: () => void; onStart: (item: DownloadItem) => void }

export function QueuePanel({ items, onClose, onStart }: Props) {
  if (items.length === 0) return null;
  return (
    <div className="queue-panel">
      <div className="queue-panel__header">
        <span>Recent</span>
        <button onClick={onClose}>✕</button>
      </div>
      {items.map(item => (
        <QueueItem
          key={item.id}
          item={item}
          onStart={item.status === 'queued' ? () => onStart(item) : undefined}
        />
      ))}
    </div>
  );
}
