import { render, screen } from '@testing-library/react';
import { describe, it, expect } from 'vitest';
import { QueueItem } from './QueueItem';
import type { DownloadItem } from '../types';

const done: DownloadItem = {
  id: '1', input: 'https://youtube.com/watch?v=x',
  track_name: 'HUMBLE.', file_path: '/music/humble.mp3',
  source: 'youtube', bpm: 150, key: 'Am', status: 'done',
};

describe('QueueItem', () => {
  it('renders track name', () => {
    render(<QueueItem item={done} />);
    expect(screen.getByText('HUMBLE.')).toBeTruthy();
  });

  it('renders BPM and key', () => {
    render(<QueueItem item={done} />);
    expect(screen.getByText('150')).toBeTruthy();
    expect(screen.getByText('Am')).toBeTruthy();
  });

  it('shows input when no track_name yet', () => {
    const queued: DownloadItem = { id: '2', input: 'search query', status: 'queued' };
    render(<QueueItem item={queued} />);
    expect(screen.getByText('search query')).toBeTruthy();
  });
});
