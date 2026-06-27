### Task 1: Scaffold — Tauri + React + floating circle window

**Files:**
- Create: project root (run scaffold command)
- Modify: `src-tauri/tauri.conf.json`
- Modify: `src-tauri/Cargo.toml`
- Create: `src/App.tsx`, `src/App.css`
- Create: `src/components/Circle.tsx`
- Create: `src/test/setup.ts`

**Interfaces:**
- Produces: running Tauri app with a 72px transparent floating circle; `<Circle />` component accepting `onDrop(input: string): void` prop

- [ ] **Step 1: Scaffold the project**

```bash
cd C:/Users/dank/dev
npm create tauri-app@latest djdrop -- --template react-ts --manager npm
cd djdrop
npm install
```

- [ ] **Step 2: Replace `src-tauri/tauri.conf.json`**

```json
{
  "$schema": "https://schema.tauri.app/config/2",
  "productName": "djdrop",
  "version": "0.1.0",
  "identifier": "com.djdrop.app",
  "build": {
    "beforeDevCommand": "npm run dev",
    "beforeBuildCommand": "npm run build",
    "devUrl": "http://localhost:1420",
    "frontendDist": "../dist"
  },
  "app": {
    "windows": [
      {
        "label": "main",
        "title": "djdrop",
        "width": 72,
        "height": 72,
        "decorations": false,
        "transparent": true,
        "alwaysOnTop": true,
        "resizable": false,
        "skipTaskbar": true
      }
    ],
    "security": { "csp": null }
  },
  "bundle": {
    "active": true,
    "targets": "all",
    "icon": ["icons/32x32.png","icons/128x128.png","icons/128x128@2x.png","icons/icon.icns","icons/icon.ico"]
  }
}
```

- [ ] **Step 3: Update `src-tauri/Cargo.toml` dependencies**

```toml
[package]
name = "djdrop"
version = "0.1.0"
edition = "2021"

[lib]
name = "djdrop_lib"
crate-type = ["lib", "cdylib", "staticlib"]

[build-dependencies]
tauri-build = { version = "2", features = [] }

[dependencies]
tauri = { version = "2", features = ["tray-icon"] }
tauri-plugin-drag = "2"
tauri-plugin-shell = "2"
tauri-plugin-keyring = "2"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
toml = "0.8"
tokio = { version = "1", features = ["full"] }
keyring = "3"
uuid = { version = "1", features = ["v4"] }
url = "2"
which = "6"
anyhow = "1"

[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 4: Create `src/App.css`**

```css
html, body, #root {
  margin: 0; padding: 0;
  width: 72px; height: 72px;
  overflow: hidden;
  background: transparent;
  user-select: none;
}

.circle {
  width: 72px; height: 72px;
  border-radius: 50%;
  background: #1a1a2e;
  display: flex; align-items: center; justify-content: center;
  color: white; font-size: 24px;
  cursor: default;
  transition: background 0.15s, box-shadow 0.15s;
  position: relative;
}

.circle[data-tauri-drag-region] { cursor: move; }

.circle.drag-over { box-shadow: 0 0 0 3px #4ade80; }

.progress-ring {
  position: absolute; top: 0; left: 0;
  width: 72px; height: 72px;
  transform: rotate(-90deg);
  pointer-events: none;
}
```

- [ ] **Step 5: Create `src/components/Circle.tsx`**

```tsx
import { useState } from 'react';

interface Props {
  onDrop: (input: string) => void;
  progress?: number; // 0–100, undefined = idle
}

export function Circle({ onDrop, progress }: Props) {
  const [isDragOver, setIsDragOver] = useState(false);

  const handleDragOver = (e: React.DragEvent) => { e.preventDefault(); setIsDragOver(true); };
  const handleDragLeave = () => setIsDragOver(false);
  const handleDrop = (e: React.DragEvent) => {
    e.preventDefault();
    setIsDragOver(false);
    const input =
      e.dataTransfer.getData('text/uri-list') ||
      e.dataTransfer.getData('text/plain');
    if (input.trim()) onDrop(input.trim());
  };

  const circumference = 2 * Math.PI * 33; // r=33
  const dashOffset = progress != null
    ? circumference * (1 - progress / 100)
    : circumference;

  return (
    <div
      className={`circle${isDragOver ? ' drag-over' : ''}`}
      data-tauri-drag-region
      onDragOver={handleDragOver}
      onDragLeave={handleDragLeave}
      onDrop={handleDrop}
    >
      {progress != null ? (
        <svg className="progress-ring" viewBox="0 0 72 72">
          <circle cx="36" cy="36" r="33" fill="none" stroke="#4ade80" strokeWidth="3"
            strokeDasharray={circumference} strokeDashoffset={dashOffset}
            style={{ transition: 'stroke-dashoffset 0.3s' }} />
        </svg>
      ) : null}
      ↓
    </div>
  );
}
```

- [ ] **Step 6: Create `src/App.tsx`**

```tsx
import './App.css';
import { Circle } from './components/Circle';

function App() {
  const handleDrop = (input: string) => {
    console.log('dropped:', input); // wired up in Task 4
  };

  return <Circle onDrop={handleDrop} />;
}

export default App;
```

- [ ] **Step 7: Install Vitest + testing library**

```bash
npm install -D vitest @vitest/ui jsdom @testing-library/react @testing-library/jest-dom
```

Add to `vite.config.ts`:
```ts
import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

export default defineConfig({
  plugins: [react()],
  test: {
    globals: true,
    environment: 'jsdom',
    setupFiles: ['./src/test/setup.ts'],
  },
});
```

- [ ] **Step 8: Create `src/test/setup.ts`**

```ts
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
```

- [ ] **Step 9: Write Circle component test**

Create `src/components/Circle.test.tsx`:
```tsx
import { render, fireEvent } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';
import { Circle } from './Circle';

describe('Circle', () => {
  it('calls onDrop with uri-list data', () => {
    const onDrop = vi.fn();
    const { container } = render(<Circle onDrop={onDrop} />);
    const el = container.firstChild as HTMLElement;

    fireEvent.dragOver(el, { dataTransfer: { getData: () => 'https://soundcloud.com/test' } });
    fireEvent.drop(el, {
      dataTransfer: { getData: (type: string) =>
        type === 'text/uri-list' ? 'https://soundcloud.com/test' : '' },
    });

    expect(onDrop).toHaveBeenCalledWith('https://soundcloud.com/test');
  });

  it('falls back to text/plain if no uri-list', () => {
    const onDrop = vi.fn();
    const { container } = render(<Circle onDrop={onDrop} />);
    const el = container.firstChild as HTMLElement;

    fireEvent.drop(el, {
      dataTransfer: { getData: (type: string) =>
        type === 'text/plain' ? 'Kendrick Lamar HUMBLE' : '' },
    });

    expect(onDrop).toHaveBeenCalledWith('Kendrick Lamar HUMBLE');
  });

  it('shows progress ring when progress prop is set', () => {
    const { container } = render(<Circle onDrop={() => {}} progress={50} />);
    expect(container.querySelector('svg')).toBeTruthy();
  });
});
```

- [ ] **Step 10: Run tests**

```bash
npm run test -- --run
```
Expected: 3 tests pass.

- [ ] **Step 11: Launch dev app to verify floating circle**

```bash
npm run tauri dev
```
Expected: a small circle appears floating over other windows, draggable by the circle body.

- [ ] **Step 12: Commit**

```bash
git add -A
git commit -m "feat: Tauri + React scaffold with floating circle window"
```

---

