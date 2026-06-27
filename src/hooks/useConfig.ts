import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { Config } from '../types';

export function useConfig() {
  const [config, setConfig] = useState<Config | null>(null);

  useEffect(() => {
    invoke<Config>('get_config').then(setConfig).catch(console.error);
  }, []);

  const saveConfig = async (updated: Config) => {
    await invoke('save_config', { config: updated });
    setConfig(updated);
  };

  return { config, saveConfig };
}
