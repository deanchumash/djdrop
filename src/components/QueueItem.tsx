import { startDrag } from '../lib/plugin-drag';
import type { DownloadItem, KeyNotation } from '../types';

const SOURCE_ICONS: Record<string, string> = {
  youtube: '▶', soundcloud: '☁', spotify: '♫', qobuz: '◈',
  bpmsupreme: 'B', clubkillers: 'C', livedjservice: 'L',
};

const CAMELOT: Record<string, string> = {
  'C': '8B', 'G': '9B', 'D': '10B', 'A': '11B', 'E': '12B', 'B': '1B',
  'F#': '2B', 'C#': '3B', 'G#': '4B', 'D#': '5B', 'A#': '6B', 'F': '7B',
  'Am': '8A', 'Em': '9A', 'Bm': '10A', 'F#m': '11A', 'C#m': '12A',
  'G#m': '1A', 'D#m': '2A', 'A#m': '3A', 'Fm': '4A', 'Cm': '5A',
  'Gm': '6A', 'Dm': '7A',
};

interface Props { item: DownloadItem; onStart?: () => void; keyNotation?: KeyNotation }

export function QueueItem({ item, onStart, keyNotation = 'musical' }: Props) {
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
      {item.key ? <span className="queue-item__key">{keyNotation === 'camelot' ? (CAMELOT[item.key] ?? item.key) : item.key}</span> : null}
      {item.status === 'queued' && onStart
        ? <button className="queue-item__start" onClick={onStart}>▶</button>
        : <span className="queue-item__status">{statusLabel}</span>
      }
    </div>
  );
}
