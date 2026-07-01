import { startDrag } from '../lib/plugin-drag';
import type { DownloadItem, KeyNotation } from '../types';

const CAMELOT: Record<string, string> = {
  'C': '8B', 'G': '9B', 'D': '10B', 'A': '11B', 'E': '12B', 'B': '1B',
  'F#': '2B', 'C#': '3B', 'G#': '4B', 'D#': '5B', 'A#': '6B', 'F': '7B',
  'Am': '8A', 'Em': '9A', 'Bm': '10A', 'F#m': '11A', 'C#m': '12A',
  'G#m': '1A', 'D#m': '2A', 'A#m': '3A', 'Fm': '4A', 'Cm': '5A',
  'Gm': '6A', 'Dm': '7A',
};

function SourceLogo({ source }: { source?: string }) {
  switch (source) {
    case 'youtube':
      return (
        <svg viewBox="0 0 20 14" width="18" height="13" aria-label="YouTube">
          <rect width="20" height="14" rx="3" fill="#FF0000" />
          <path d="M8 3.5L14.5 7L8 10.5Z" fill="white" />
        </svg>
      );
    case 'soundcloud':
      return (
        <svg viewBox="0 0 24 16" width="18" height="12" aria-label="SoundCloud">
          <rect width="2.5" height="8"  x="0"    y="8"  rx="1.2" fill="#FF5500" />
          <rect width="2.5" height="11" x="4"    y="5"  rx="1.2" fill="#FF5500" />
          <rect width="2.5" height="14" x="8"    y="2"  rx="1.2" fill="#FF5500" />
          <rect width="2.5" height="11" x="12"   y="5"  rx="1.2" fill="#FF5500" />
          <rect width="2.5" height="8"  x="16"   y="8"  rx="1.2" fill="#FF5500" />
          <rect width="2.5" height="5"  x="20"   y="11" rx="1.2" fill="#FF5500" />
        </svg>
      );
    case 'spotify':
      return (
        <svg viewBox="0 0 24 24" width="14" height="14" aria-label="Spotify">
          <circle cx="12" cy="12" r="12" fill="#1DB954" />
          <path d="M7 9.5 Q12 7.5 17 9.5"   stroke="white" strokeWidth="2"   strokeLinecap="round" fill="none" />
          <path d="M7 13  Q12 11  16 13"     stroke="white" strokeWidth="1.7" strokeLinecap="round" fill="none" />
          <path d="M7 16  Q12 14.5 15 16"   stroke="white" strokeWidth="1.4" strokeLinecap="round" fill="none" />
        </svg>
      );
    case 'qobuz':
      return (
        <svg viewBox="0 0 24 24" width="14" height="14" aria-label="Qobuz">
          <circle cx="12" cy="12" r="12" fill="#003D8C" />
          <circle cx="11" cy="11" r="5"  stroke="white" strokeWidth="2" fill="none" />
          <path d="M14.5 14.5L18 18" stroke="white" strokeWidth="2" strokeLinecap="round" />
        </svg>
      );
    case 'bpmsupreme':
      return <span className="queue-item__source-text" style={{ color: '#a855f7' }}>B</span>;
    case 'clubkillers':
      return <span className="queue-item__source-text" style={{ color: '#f59e0b' }}>C</span>;
    case 'livedjservice':
      return <span className="queue-item__source-text" style={{ color: '#06b6d4' }}>L</span>;
    default:
      return <span className="queue-item__source-text">?</span>;
  }
}

interface Props { item: DownloadItem; onStart?: () => void; keyNotation?: KeyNotation }

export function QueueItem({ item, onStart, keyNotation = 'musical' }: Props) {
  const handleDragStart = async (e: React.DragEvent) => {
    if (!item.file_path) return;
    e.preventDefault();
    await startDrag({ items: [item.file_path] });
  };

  const statusLabel = {
    queued:     '…',
    downloading: '',
    analyzing:  'analyzing',
    done:       '',
    error:      '✕',
  }[item.status];

  return (
    <div
      className={`queue-item queue-item--${item.status}`}
      draggable={!!item.file_path}
      onDragStart={handleDragStart}
    >
      <span className="queue-item__source">
        <SourceLogo source={item.source} />
      </span>
      <span className="queue-item__name">{item.track_name ?? item.input}</span>
      {item.bpm ? <span className="queue-item__bpm">{item.bpm}</span> : null}
      {item.key
        ? <span className="queue-item__key">
            {keyNotation === 'camelot' ? (CAMELOT[item.key] ?? item.key) : item.key}
          </span>
        : null}
      {item.status === 'downloading'
        ? <span className="queue-item__spinner" aria-label="downloading" />
        : item.status === 'queued' && onStart
          ? <button className="queue-item__start" onClick={onStart}>▶</button>
          : <span className="queue-item__status">{statusLabel}</span>}
    </div>
  );
}
