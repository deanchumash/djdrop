import { useState } from 'react';

interface Props {
  onDrop: (input: string) => void;
  progress?: number; // 0–100, undefined = idle
}

export function Circle({ onDrop, progress }: Props) {
  const [isDragOver, setIsDragOver] = useState(false);

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
      data-tauri-drag-region
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
    </div>
  );
}
