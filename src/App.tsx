import './App.css';
import { useState } from 'react';
import { Circle } from './components/Circle';
import { SearchBar } from './components/SearchBar';
import { QueuePanel } from './components/QueuePanel';
import { SettingsPanel } from './components/SettingsPanel';
import { useDownloads } from './hooks/useDownloads';
import { useConfig } from './hooks/useConfig';

function App() {
  const { downloads, activeProgress, startDownload, queueOnly, triggerDownload } = useDownloads();
  const { config } = useConfig();
  const [showSearch, setShowSearch] = useState(false);
  const [showQueue, setShowQueue] = useState(false);
  const [showSettings, setShowSettings] = useState(false);

  const handleDrop = (input: string) => {
    if (config?.auto_download !== false) {
      startDownload(input);
    } else {
      queueOnly(input);
    }
  };

  const handleSearch = (query: string) => {
    if (config?.auto_download !== false) {
      startDownload(query);
    } else {
      queueOnly(query);
    }
  };

  return (
    <div className="app-root" onContextMenu={e => { e.preventDefault(); setShowSettings(v => !v); }}>
      {showSettings && <SettingsPanel onClose={() => setShowSettings(false)} />}
      {showQueue && (
        <QueuePanel
          items={downloads}
          onClose={() => setShowQueue(false)}
          onStart={item => triggerDownload(item)}
        />
      )}
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
