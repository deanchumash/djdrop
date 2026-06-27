import { useState, useEffect, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { DownloadItem, ProgressPayload, DonePayload, ErrorPayload, AnalysisDonePayload } from '../types';

export function useDownloads() {
  const [downloads, setDownloads] = useState<DownloadItem[]>([]);
  const [activeProgress, setActiveProgress] = useState<number | undefined>(undefined);

  useEffect(() => {
    const unlisteners = [
      listen<ProgressPayload>('download:progress', ({ payload }) => {
        setActiveProgress(payload.percent);
        setDownloads(prev => prev.map(d =>
          d.id === payload.id ? { ...d, progress: payload.percent, status: 'downloading' } : d
        ));
      }),
      listen<DonePayload>('download:done', ({ payload }) => {
        setActiveProgress(undefined);
        setDownloads(prev => prev.map(d =>
          d.id === payload.id
            ? { ...d, file_path: payload.file_path, track_name: payload.track_name, source: payload.source, status: 'analyzing' }
            : d
        ));
      }),
      listen<ErrorPayload>('download:error', ({ payload }) => {
        setActiveProgress(undefined);
        setDownloads(prev => prev.map(d =>
          d.id === payload.id ? { ...d, status: 'error', error: payload.message } : d
        ));
      }),
      listen<AnalysisDonePayload>('analysis:done', ({ payload }) => {
        setDownloads(prev => prev.map(d =>
          d.id === payload.id ? { ...d, bpm: payload.bpm, key: payload.key, status: 'done' } : d
        ));
      }),
    ];
    return () => { unlisteners.forEach(p => p.then(fn => fn())); };
  }, []);

  const startDownload = useCallback(async (input: string) => {
    const id = crypto.randomUUID();
    const item: DownloadItem = { id, input, status: 'queued' };
    setDownloads(prev => [item, ...prev]);
    await invoke('start_download', { id, input });
  }, []);

  return { downloads, activeProgress, startDownload };
}
