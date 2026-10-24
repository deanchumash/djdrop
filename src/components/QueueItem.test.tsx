import { fireEvent, render, screen } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';
import { QueueItem } from './QueueItem';
import type { DownloadItem } from '../types';

const item: DownloadItem = { id: '1', input: 'search query', status: 'queued' };

describe('QueueItem', () => {
  it('keeps a manual item startable only until it has been initiated', () => {
    const onStart = vi.fn();
    const { rerender } = render(<QueueItem item={item} onStart={onStart} />);
    fireEvent.click(screen.getByRole('button', { name: 'Start download for search query' }));
    expect(onStart).toHaveBeenCalledOnce();

    rerender(<QueueItem item={{ ...item, status: 'pending' }} />);
    expect(screen.queryByRole('button')).toBeNull();
    expect(screen.getByText('Starting')).toBeTruthy();
    expect(screen.queryByRole('progressbar')).toBeNull();
  });

  it('shows unknown progress without a numeric value until actual progress arrives', () => {
    const { rerender } = render(<QueueItem item={{ ...item, status: 'downloading' }} />);
    const progress = screen.getByRole('progressbar');
    expect(progress.getAttribute('aria-valuenow')).toBeNull();
    expect(screen.getByText('Downloading')).toBeTruthy();
    expect(screen.queryByText(/%/)).toBeNull();

    rerender(<QueueItem item={{ ...item, status: 'downloading', progress: 0 }} />);
    expect(screen.getByRole('progressbar').getAttribute('aria-valuenow')).toBe('0');
    expect(screen.getByText('Downloading 0%')).toBeTruthy();

    rerender(<QueueItem item={{ ...item, status: 'downloading', progress: 67 }} />);
    expect(screen.getByRole('progressbar').getAttribute('aria-valuenow')).toBe('67');
    expect(screen.getByText('Downloading 67%')).toBeTruthy();

    rerender(<QueueItem item={{ ...item, status: 'analyzing' }} />);
    expect(screen.queryByRole('progressbar')).toBeNull();
    expect(screen.getByText('Analyzing')).toBeTruthy();
  });

  it('shows the full failure reason without hover and removes it after completion', () => {
    const message = 'no audio file found in output dir: YT-DLP download error - https://soundcloud.com/ennedyalone/fun-1';
    const { rerender } = render(<QueueItem item={{ ...item, status: 'error', error: message }} />);
    expect(screen.getByRole('alert').textContent).toBe(message);
    expect(screen.queryByRole('progressbar')).toBeNull();

    rerender(<QueueItem item={{ ...item, status: 'done' }} />);
    expect(screen.queryByRole('alert')).toBeNull();
  });
});
