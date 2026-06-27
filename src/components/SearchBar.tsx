import { useState, useRef, useEffect } from 'react';

interface Props { onSearch: (query: string) => void; onClose: () => void }

export function SearchBar({ onSearch, onClose }: Props) {
  const [query, setQuery] = useState('');
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => { inputRef.current?.focus(); }, []);

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter' && query.trim()) { onSearch(query.trim()); onClose(); }
    if (e.key === 'Escape') onClose();
  };

  return (
    <div className="search-bar">
      <input
        ref={inputRef}
        value={query}
        onChange={e => setQuery(e.target.value)}
        onKeyDown={handleKeyDown}
        placeholder="Search or paste URL…"
      />
    </div>
  );
}
