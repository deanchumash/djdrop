import { useState, useEffect, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { DownloadItem, ProgressPayload, DonePayload, ErrorPayload, AnalysisDonePayload } from '../types';

export function useDownloads() {
  const [downloads, setDownloads] = useState<DownloadItem[]>([]);
  // The legacy React circle consumes a single percentage; derive it from a
  // still-running item so another download's completion cannot clear it.
  const activeProgress = downloads.find(d => d.status === 'downloading' && d.progress !== undefined)?.progress;

  useEffect(() => {
    const unlisteners = [
      // Native-circle drops are already accepted by the backend, not manually queued.
      listen<{ id: string; input: string }>('download:queued', ({ payload }) => {
        setDownloads(prev => {
          if (prev.some(d => d.id === payload.id)) return prev;
          return [{ id: payload.id, input: payload.input, status: 'pending' }, ...prev];
        });
      }),
      listen<{ id: string }>('download:started', ({ payload }) => {
        setDownloads(prev => prev.map(d =>
          d.id === payload.id && d.status === 'pending' ? { ...d, status: 'downloading' } : d
        ));
      }),
      listen<ProgressPayload>('download:progress', ({ payload }) => {
        setDownloads(prev => prev.map(d =>
          d.id === payload.id && (d.status === 'pending' || d.status === 'downloading')
            ? { ...d, progress: payload.percent, status: 'downloading' }
            : d
        ));
      }),
      listen<DonePayload>('download:done', ({ payload }) => {
        setDownloads(prev => prev.map(d =>
          d.id === payload.id
            ? {
                ...d,
                file_path: payload.file_path, track_name: payload.track_name, source: payload.source,
                format: payload.format, bitrate: payload.bitrate ?? undefined, status: 'analyzing',
              }
            : d
        ));
      }),
      listen<ErrorPayload>('download:error', ({ payload }) => {
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
    setDownloads(prev => [{ id, input, status: 'pending' }, ...prev]);
    try {
      await invoke('start_download', { id, input });
    } catch (error) {
      setDownloads(prev => prev.map(d =>
        d.id === id ? { ...d, status: 'error', error: String(error) } : d
      ));
    }
  }, []);

  const queueOnly = useCallback((input: string) => {
    const id = crypto.randomUUID();
    setDownloads(prev => [{ id, input, status: 'queued' }, ...prev]);
  }, []);

  const triggerDownload = useCallback(async (item: DownloadItem) => {
    setDownloads(prev => prev.map(d =>
      d.id === item.id && d.status === 'queued' ? { ...d, status: 'pending' } : d
    ));
    try {
      await invoke('start_download', { id: item.id, input: item.input });
    } catch (error) {
      setDownloads(prev => prev.map(d =>
        d.id === item.id ? { ...d, status: 'error', error: String(error) } : d
      ));
    }
  }, []);

  return { downloads, activeProgress, startDownload, queueOnly, triggerDownload };
}
