import './App.css';
import { useState } from 'react';
import { Circle } from './components/Circle';
import { SearchBar } from './components/SearchBar';
import { QueuePanel } from './components/QueuePanel';
import { SettingsPanel } from './components/SettingsPanel';
import { useDownloads } from './hooks/useDownloads';

function App() {
  const { downloads, activeProgress, startDownload } = useDownloads();
  const [showSearch, setShowSearch] = useState(false);
  const [showQueue, setShowQueue] = useState(false);
  const [showSettings, setShowSettings] = useState(false);

  const handleDrop = (input: string) => startDownload(input);
  const handleSearch = (query: string) => startDownload(query);

  return (
    <div className="app-root" onContextMenu={e => { e.preventDefault(); setShowSettings(v => !v); }}>
      {showSettings && <SettingsPanel onClose={() => setShowSettings(false)} />}
      {showQueue && <QueuePanel items={downloads} onClose={() => setShowQueue(false)} />}
      {showSearch && <SearchBar onSearch={handleSearch} onClose={() => setShowSearch(false)} />}
      <Circle
        onDrop={handleDrop}
        progress={activeProgress}
        onClickBody={() => setShowQueue(v => !v)}
        onClickSearch={() => setShowSearch(v => !v)}
      />
    </div>
  );
}

export default App;
