import { QueueItem } from './QueueItem';
import type { DownloadItem } from '../types';

interface Props { items: DownloadItem[]; onClose: () => void }

export function QueuePanel({ items, onClose }: Props) {
  if (items.length === 0) return null;
  return (
    <div className="queue-panel">
      <div className="queue-panel__header">
        <span>Recent</span>
        <button onClick={onClose}>✕</button>
      </div>
      {items.map(item => <QueueItem key={item.id} item={item} />)}
    </div>
  );
}
