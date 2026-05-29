import { useEffect, useMemo, useState } from 'react';
import type { GraphData, CheckpointData } from './types';
import { Timeline } from './Timeline';
import { Detail } from './Detail';

// Mock for dev fallback
const MOCK: GraphData = {
  project_name: 'demo',
  head_save_id: null,
  checkpoints: [
    {
      id: 'cp-1', title: 'Initial setup',
      prompt: 'Create a new Rust project with SQLite storage for checkpoints',
      response: null, agent: 'claude-opus-4', session_id: null,
      parent_save_id: null, created_at: new Date(Date.now() - 3600000 * 6).toISOString(),
      private: false, is_head: false, is_main_chain: true,
      saves: [
        { id: 's-1a', sequence: 0, message: 'initial state', file_count: 4, root_hash: 'abc123', diff_added: 0, diff_modified: 0, diff_removed: 0, created_at: new Date(Date.now() - 3600000 * 6).toISOString() },
        { id: 's-1b', sequence: 1, message: 'add Cargo.toml deps', file_count: 5, root_hash: 'def456', diff_added: 0, diff_modified: 0, diff_removed: 0, created_at: new Date(Date.now() - 3600000 * 5).toISOString() },
      ],
    },
    {
      id: 'cp-2', title: 'Add checkpoint command',
      prompt: 'Implement lupe checkpoint with SQLite storage and UUID v7',
      response: 'Added checkpoint command with SQLite backend. Uses UUID v7 for time-ordered IDs.',
      agent: 'claude-opus-4', session_id: null,
      parent_save_id: 's-1b', created_at: new Date(Date.now() - 3600000 * 4).toISOString(),
      private: false, is_head: false, is_main_chain: true,
      saves: [
        { id: 's-2a', sequence: 0, message: 'initial state', file_count: 6, root_hash: 'ghi789', diff_added: 0, diff_modified: 0, diff_removed: 0, created_at: new Date(Date.now() - 3600000 * 4).toISOString() },
        { id: 's-2b', sequence: 1, message: 'add checkpoint table', file_count: 7, root_hash: 'jkl012', diff_added: 0, diff_modified: 0, diff_removed: 0, created_at: new Date(Date.now() - 3600000 * 3.5).toISOString() },
      ],
    },
    {
      id: 'cp-dead', title: 'Try postgres instead',
      prompt: 'Switch to postgres backend for scalability',
      response: null, agent: 'claude-sonnet-4', session_id: null,
      parent_save_id: 's-1b', created_at: new Date(Date.now() - 3600000 * 4.5).toISOString(),
      private: false, is_head: false, is_main_chain: false,
      saves: [
        { id: 's-d1', sequence: 0, message: 'initial state', file_count: 5, root_hash: 'xyz999', diff_added: 0, diff_modified: 0, diff_removed: 0, created_at: new Date(Date.now() - 3600000 * 4.5).toISOString() },
      ],
    },
    {
      id: 'cp-3', title: 'Add web graph command',
      prompt: 'Add lupe graph --web that opens a beautiful React Flow graph in the browser',
      response: 'Implemented graph --web with axum server, React Flow frontend, checkpoint nodes with saves embedded.',
      agent: 'claude-opus-4', session_id: null,
      parent_save_id: 's-2b', created_at: new Date(Date.now() - 3600000 * 2).toISOString(),
      private: false, is_head: true, is_main_chain: true,
      saves: [
        { id: 's-3a', sequence: 0, message: 'initial state', file_count: 8, root_hash: 'mno345', diff_added: 0, diff_modified: 0, diff_removed: 0, created_at: new Date(Date.now() - 3600000 * 2).toISOString() },
        { id: 's-3b', sequence: 1, message: 'graph renders tree', file_count: 8, root_hash: 'pqr678', diff_added: 0, diff_modified: 0, diff_removed: 0, created_at: new Date(Date.now() - 3600000 * 1).toISOString() },
      ],
    },
  ],
};

export default function App() {
  const [data, setData] = useState<GraphData | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);

  useEffect(() => {
    fetch('/api/graph')
      .then(r => { if (!r.ok) throw new Error(); return r.json(); })
      .then((d: GraphData) => {
        setData(d);
        // Auto-select HEAD
        const head = d.checkpoints.find(c => c.is_head);
        if (head) setSelectedId(head.id);
      })
      .catch(() => {
        setData(MOCK);
        setSelectedId('cp-3');
      });
  }, []);

  const selected = useMemo<CheckpointData | null>(() => {
    if (!data || !selectedId) return null;
    return data.checkpoints.find(c => c.id === selectedId) ?? null;
  }, [data, selectedId]);

  if (!data) {
    return (
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'center', height: '100%', color: '#b0aac8', fontSize: 13 }}>
        loading…
      </div>
    );
  }

  return (
    <div style={{ display: 'flex', height: '100%', width: '100%' }}>
      {/* Top bar */}
      <div style={{
        position: 'absolute', top: 0, left: 0, right: 0, zIndex: 10, height: 36,
        background: '#ffffff',
        borderBottom: '1px solid #e4e0f0',
        display: 'flex', alignItems: 'center',
        padding: '0 16px', gap: 8,
        pointerEvents: 'none',
      }}>
        <span style={{ fontSize: 11, color: '#6058e8', fontWeight: 700 }}>◆ lupe</span>
        <span style={{ fontSize: 11, color: '#ccc8e0' }}>/</span>
        <span style={{ fontSize: 11, color: '#5a5678' }}>{data.project_name}</span>
        <span style={{ fontSize: 10, color: '#b0aac8', marginLeft: 'auto' }}>
          {data.checkpoints.filter(c => c.is_main_chain).length} checkpoints
          {data.checkpoints.filter(c => !c.is_main_chain).length > 0 &&
            ` · ${data.checkpoints.filter(c => !c.is_main_chain).length} dead`}
        </span>
      </div>

      {/* Layout: timeline left, detail right */}
      <div style={{ display: 'flex', width: '100%', height: '100%', paddingTop: 36 }}>
        <Timeline
          checkpoints={data.checkpoints}
          selectedId={selectedId}
          onSelect={setSelectedId}
        />
        <Detail checkpoint={selected} />
      </div>
    </div>
  );
}
