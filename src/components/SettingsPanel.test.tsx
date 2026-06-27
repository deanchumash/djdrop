import { render, screen } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { SettingsPanel } from './SettingsPanel';

vi.mocked(invoke).mockResolvedValue({
  output_dir: 'C:/Music/djdrop', auto_download: true,
  search_mode: 'best_match', search_priority: 'pools',
  pools: {
    bpmsupreme: { username_ref: '', password_ref: '' },
    clubkillers: { username_ref: '', password_ref: '' },
    livedjservice: { username_ref: '', password_ref: '' },
    qobuz: { username_ref: '', password_ref: '' },
  },
});

describe('SettingsPanel', () => {
  it('renders pool credential fields', async () => {
    render(<SettingsPanel onClose={() => {}} />);
    // Wait for config load
    await screen.findByText('bpmsupreme');
    expect(screen.getByText('clubkillers')).toBeTruthy();
    expect(screen.getByText('livedjservice')).toBeTruthy();
    expect(screen.getByText('qobuz')).toBeTruthy();
  });
});
