import { useState } from 'react';
import { getCurrentWindow } from '@tauri-apps/api/window';

interface Props {
  onDrop: (input: string) => void;
  progress?: number;
  onClickBody?: () => void;
}

const R = 47;
const CIRCUMFERENCE = 2 * Math.PI * R;

export function Circle({ onDrop, progress, onClickBody }: Props) {
  const [isDragOver, setIsDragOver] = useState(false);

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
    const input = e.dataTransfer.getData('text/uri-list') || e.dataTransfer.getData('text/plain');
    if (input.trim()) onDrop(input.trim());
  };

  return (
    <div
      className={`circle${isDragOver ? ' drag-over' : ''}`}
      onMouseDown={handleMouseDown}
      onClick={onClickBody}
      onDragOver={handleDragOver}
      onDragLeave={handleDragLeave}
      onDrop={handleDrop}
    >
      {progress != null && (
        <svg className="progress-ring" viewBox="0 0 100 100">
          <circle
            cx="50" cy="50" r={R}
            fill="none"
            stroke="#4ade80"
            strokeWidth="4"
            strokeDasharray={CIRCUMFERENCE}
            strokeDashoffset={CIRCUMFERENCE * (1 - progress / 100)}
            style={{ transition: 'stroke-dashoffset 0.3s' }}
          />
        </svg>
      )}
      <span className="circle__icon">↓</span>
    </div>
  );
}
