import './App.css';
import { invoke } from '@tauri-apps/api/core';
import { Circle } from './components/Circle';
import { useDownloads } from './hooks/useDownloads';
import { useConfig } from './hooks/useConfig';

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
    <Circle
      onDrop={handleDrop}
      progress={activeProgress}
      onClickBody={() => invoke('toggle_panels')}
    />
  );
}

export default App;
