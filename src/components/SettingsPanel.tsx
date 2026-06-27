import { useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useConfig } from '../hooks/useConfig';
import type { Config } from '../types';

interface Props { onClose: () => void }

export function SettingsPanel({ onClose }: Props) {
  const { config, saveConfig } = useConfig();
  const [saving, setSaving] = useState(false);

  if (!config) return <div className="settings-panel">Loading…</div>;

  const update = async (patch: Partial<Config>) => {
    setSaving(true);
    await saveConfig({ ...config, ...patch });
    setSaving(false);
  };

  const handleCredentialSave = async (pool: string) => {
    const username = (document.getElementById(`${pool}-user`) as HTMLInputElement)?.value;
    const password = (document.getElementById(`${pool}-pass`) as HTMLInputElement)?.value;
    if (!username || !password) return;
    await invoke('save_credential', { pool, username, password });
  };

  const pools = ['bpmsupreme', 'clubkillers', 'livedjservice', 'qobuz'] as const;

  return (
    <div className="settings-panel">
      <div className="settings-panel__header">
        <span>Settings</span>
        <button onClick={onClose}>✕</button>
      </div>

      <label>Output folder
        <input value={config.output_dir} onChange={e => update({ output_dir: e.target.value })} />
      </label>

      <label>
        <input type="checkbox" checked={config.auto_download}
          onChange={e => update({ auto_download: e.target.checked })} />
        Auto-download
      </label>

      <label>Search behavior
        <select value={config.search_mode} onChange={e => update({ search_mode: e.target.value as Config['search_mode'] })}>
          <option value="best_match">Best match</option>
          <option value="show_results">Show results</option>
        </select>
      </label>

      <label>Search source priority
        <select value={config.search_priority} onChange={e => update({ search_priority: e.target.value as Config['search_priority'] })}>
          <option value="pools">Pools first</option>
          <option value="youtube">YouTube first</option>
        </select>
      </label>

      <hr />
      <p className="settings-panel__section">Credentials — paste op:// references or direct values</p>

      {pools.map(pool => {
        const refs = config.pools[pool];
        return (
          <div key={pool} className="settings-panel__pool">
            <strong>{pool}</strong>
            <input id={`${pool}-user`} defaultValue={refs.username_ref} placeholder="op://Vault/Item/username" />
            <input id={`${pool}-pass`} defaultValue={refs.password_ref} placeholder="op://Vault/Item/password" type="password" />
            <button onClick={() => {
              const u = (document.getElementById(`${pool}-user`) as HTMLInputElement)?.value;
              const p = (document.getElementById(`${pool}-pass`) as HTMLInputElement)?.value;
              update({ pools: { ...config.pools, [pool]: { username_ref: u, password_ref: p } } });
            }}>Save ref</button>
            <button onClick={() => handleCredentialSave(pool)}>Save to keychain</button>
          </div>
        );
      })}
      {saving && <span className="settings-panel__saving">Saving…</span>}
    </div>
  );
}
