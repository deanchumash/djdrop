import { useState } from 'react';
import { getCurrentWindow } from '@tauri-apps/api/window';

interface Props {
  onDrop: (input: string) => void;
  progress?: number; // 0–100, undefined = idle
  onClickBody?: () => void;
  onClickSearch?: () => void;
}

export function Circle({ onDrop, progress, onClickBody, onClickSearch }: Props) {
  const [isDragOver, setIsDragOver] = useState(false);

  // Only start window drag once mouse moves; plain click still fires onClick
  const handleMouseDown = (e: React.MouseEvent) => {
    if (e.button !== 0) return;
    const startX = e.clientX;
    const startY = e.clientY;

    const onMove = (me: MouseEvent) => {
      if (Math.abs(me.clientX - startX) > 4 || Math.abs(me.clientY - startY) > 4) {
        cleanup();
        getCurrentWindow().startDragging();
      }
    };
    const onUp = () => cleanup();
    const cleanup = () => {
      document.removeEventListener('mousemove', onMove);
      document.removeEventListener('mouseup', onUp);
    };
    document.addEventListener('mousemove', onMove);
    document.addEventListener('mouseup', onUp);
  };

  const handleDragOver = (e: React.DragEvent) => { e.preventDefault(); setIsDragOver(true); };
  const handleDragLeave = () => setIsDragOver(false);
  const handleDrop = (e: React.DragEvent) => {
    e.preventDefault();
    setIsDragOver(false);
    const input =
      e.dataTransfer.getData('text/uri-list') ||
      e.dataTransfer.getData('text/plain');
    if (input.trim()) onDrop(input.trim());
  };

  const circumference = 2 * Math.PI * 33; // r=33
  const dashOffset = progress != null
    ? circumference * (1 - progress / 100)
    : circumference;

  return (
    <div
      className={`circle${isDragOver ? ' drag-over' : ''}`}
      onMouseDown={handleMouseDown}
      onClick={onClickBody}
      onDragOver={handleDragOver}
      onDragLeave={handleDragLeave}
      onDrop={handleDrop}
    >
      {progress != null ? (
        <svg className="progress-ring" viewBox="0 0 72 72">
          <circle cx="36" cy="36" r="33" fill="none" stroke="#4ade80" strokeWidth="3"
            strokeDasharray={circumference} strokeDashoffset={dashOffset}
            style={{ transition: 'stroke-dashoffset 0.3s' }} />
        </svg>
      ) : null}
      ↓
      <button
        className="search-btn"
        onMouseDown={e => e.stopPropagation()}
        onClick={e => { e.stopPropagation(); onClickSearch?.(); }}
      >🔍</button>
    </div>
  );
}
