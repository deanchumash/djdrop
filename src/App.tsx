import './App.css';
import { invoke } from '@tauri-apps/api/core';
import { Circle } from './components/Circle';
import { useDownloads } from './hooks/useDownloads';
import { useConfig } from './hooks/useConfig';

// Circle window: just the 72x72 drop target. All panels live in a separate window.
function App() {
  const { activeProgress, startDownload, queueOnly } = useDownloads();
  const { config } = useConfig();

  const handleDrop = (input: string) => {
    if (config?.auto_download !== false) {
      startDownload(input);
    } else {
      queueOnly(input);
    }
  };

  return (
    <div className="circle-root">
      <Circle
        onDrop={handleDrop}
        progress={activeProgress}
        onClickBody={() => invoke('toggle_panels')}
      />
    </div>
  );
}

export default App;
