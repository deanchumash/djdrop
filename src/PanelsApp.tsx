import './App.css';
import { useState, useEffect } from 'react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { invoke } from '@tauri-apps/api/core';
import { SearchBar } from './components/SearchBar';
import { QueuePanel } from './components/QueuePanel';
import { SettingsPanel } from './components/SettingsPanel';
import { useDownloads } from './hooks/useDownloads';
import { useConfig } from './hooks/useConfig';

export function PanelsApp() {
  const { downloads, startDownload, queueOnly, triggerDownload } = useDownloads();
  const { config } = useConfig();
  const [showSearch, setShowSearch] = useState(false);
  const [showSettings, setShowSettings] = useState(false);

  // Hide the panels window when it loses focus
  useEffect(() => {
    const win = getCurrentWindow();
    let unlisten: (() => void) | undefined;
    win.onFocusChanged(({ payload: focused }) => {
      if (!focused) win.hide();
    }).then(fn => { unlisten = fn; });
    return () => { unlisten?.(); };
  }, []);

  // Escape to close
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') getCurrentWindow().hide();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, []);

  const handleSearch = (query: string) => {
    if (config?.auto_download !== false) {
      startDownload(query);
    } else {
      queueOnly(query);
    }
    setShowSearch(false);
  };

  const hide = () => getCurrentWindow().hide();

  return (
    <div
      className="panels-root"
      onContextMenu={e => { e.preventDefault(); setShowSettings(v => !v); }}
    >
      <div className="panels-toolbar">
        <span className="panels-toolbar__title">djdrop</span>
        <button
          className={`panels-toolbar__btn${showSearch ? ' active' : ''}`}
          onClick={() => { setShowSearch(v => !v); setShowSettings(false); }}
          title="Search"
        >🔍</button>
        <button
          className={`panels-toolbar__btn${showSettings ? ' active' : ''}`}
          onClick={() => { setShowSettings(v => !v); setShowSearch(false); }}
          title="Settings"
        >⚙</button>
        <button className="panels-toolbar__btn" onClick={() => invoke('quit_app')} title="Quit">✕</button>
      </div>

      {showSearch && (
        <SearchBar onSearch={handleSearch} onClose={() => setShowSearch(false)} />
      )}

      <div className="panels-content">
        {showSettings ? (
          <SettingsPanel onClose={() => setShowSettings(false)} />
        ) : (
          <QueuePanel
            items={downloads}
            onClose={hide}
            onStart={item => triggerDownload(item)}
            keyNotation={config?.key_notation}
          />
        )}
      </div>
    </div>
  );
}
