import { renderHook } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { useConfig } from './useConfig';

describe('useConfig', () => {
  it('returns null config initially before invoke resolves', () => {
    vi.mocked(invoke).mockReturnValue(new Promise(() => {})); // never resolves
    const { result } = renderHook(() => useConfig());
    expect(result.current.config).toBeNull();
  });

  it('exposes a saveConfig function', () => {
    vi.mocked(invoke).mockReturnValue(new Promise(() => {}));
    const { result } = renderHook(() => useConfig());
    expect(typeof result.current.saveConfig).toBe('function');
  });
});
