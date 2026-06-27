import '@testing-library/jest-dom';
import { vi } from 'vitest';

// Mock Tauri APIs — not available in jsdom
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
}));
vi.mock('@tauri-apps/plugin-drag', () => ({
  startDrag: vi.fn(),
}));
