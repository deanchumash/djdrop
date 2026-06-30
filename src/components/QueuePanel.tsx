import { QueueItem } from './QueueItem';
import type { DownloadItem } from '../types';

interface Props { items: DownloadItem[]; onClose: () => void; onStart: (item: DownloadItem) => void }

export function QueuePanel({ items, onStart }: Props) {
  if (items.length === 0) {
    return (
      <div className="queue-panel">
        <p className="queue-panel__empty">Drop a URL or link on the circle to download</p>
      </div>
    );
  }
  return (
    <div className="queue-panel">
      <div className="queue-panel__header">
        <span>Recent</span>
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
