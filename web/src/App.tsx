import { useEffect, useMemo, useState } from 'react';
import type { GraphData, CheckpointData } from './types';
import { Timeline } from './Timeline';
import { Detail } from './Detail';

// Mock for dev fallback
const MOCK: GraphData = {
  project_name: 'demo',
  head_checkpoint_id: 'cp-3',
  branches: [
    { name: 'main', head_checkpoint_id: 'cp-3', created_at: new Date(Date.now() - 3600000 * 6).toISOString(), updated_at: new Date(Date.now() - 3600000 * 2).toISOString() },
    { name: 'try-postgres', head_checkpoint_id: 'cp-branch', created_at: new Date(Date.now() - 3600000 * 4.5).toISOString(), updated_at: new Date(Date.now() - 3600000 * 4.5).toISOString() },
  ],
  checkpoints: [
    {
      id: 'cp-1', title: 'Initial setup',
      prompt: 'Create a new Rust project with SQLite storage for checkpoints',
      response: null, agent: 'claude-opus-4', session_id: null,
      parent_checkpoint_id: null, root_hash: 'abc123', file_count: 4,
      created_at: new Date(Date.now() - 3600000 * 6).toISOString(),
      private: false, is_head: false, branch_name: 'main',
      diff_added: 4, diff_modified: 0, diff_removed: 0,
    },
    {
      id: 'cp-2', title: 'Add checkpoint command',
      prompt: 'Implement lupe checkpoint with SQLite storage and UUID v7',
      response: 'Added checkpoint command with SQLite backend. Uses UUID v7 for time-ordered IDs.',
      agent: 'claude-opus-4', session_id: null,
      parent_checkpoint_id: 'cp-1', root_hash: 'ghi789', file_count: 6,
      created_at: new Date(Date.now() - 3600000 * 4).toISOString(),
      private: false, is_head: false, branch_name: 'main',
      diff_added: 1, diff_modified: 2, diff_removed: 0,
    },
    {
      id: 'cp-branch', title: 'Try postgres instead',
      prompt: 'Switch to postgres backend for scalability',
      response: null, agent: 'claude-sonnet-4', session_id: null,
      parent_checkpoint_id: 'cp-1', root_hash: 'xyz999', file_count: 5,
      created_at: new Date(Date.now() - 3600000 * 4.5).toISOString(),
      private: false, is_head: false, branch_name: 'try-postgres',
      diff_added: 1, diff_modified: 1, diff_removed: 0,
    },
    {
      id: 'cp-3', title: 'Add web graph command',
      prompt: 'Add lupe graph --web that opens a beautiful graph in the browser',
      response: 'Implemented graph --web with axum server and React frontend.',
      agent: 'claude-opus-4', session_id: null,
      parent_checkpoint_id: 'cp-2', root_hash: 'mno345', file_count: 8,
      created_at: new Date(Date.now() - 3600000 * 2).toISOString(),
      private: false, is_head: true, branch_name: 'main',
      diff_added: 3, diff_modified: 1, diff_removed: 0,
    },
  ],
};

export default function App() {
  const [data, setData] = useState<GraphData | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);

  useEffect(() => {
    let initial = true;

    function fetchGraph() {
      fetch('/api/graph')
        .then(r => { if (!r.ok) throw new Error(); return r.json(); })
        .then((d: GraphData) => {
          setData(d);
          if (initial) {
            const head = d.checkpoints.find(c => c.is_head);
            if (head) setSelectedId(head.id);
            initial = false;
          }
        })
        .catch(() => {
          if (initial) {
            setData(MOCK);
            setSelectedId('cp-3');
            initial = false;
          }
        });
    }

    fetchGraph();

    // Real-time updates via SSE — server pushes on every new checkpoint
    const es = new EventSource('/api/events');
    es.addEventListener('checkpoint', () => fetchGraph());
    es.onerror = () => fetchGraph();

    // Heartbeat poll — catches anything SSE misses (reconnects, missed events)
    const interval = setInterval(fetchGraph, 5000);

    return () => { es.close(); clearInterval(interval); };
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
          {data.checkpoints.length} checkpoints
          {data.branches.length > 0 &&
            ` · ${data.branches.length} branch${data.branches.length !== 1 ? 'es' : ''}`}
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
