import { useEffect, useState } from 'react';
import type { CheckpointData } from './types';
import { FileDiffView, type FileDiff } from './FileDiff';

function timeAgo(iso: string) {
  const s = Math.floor((Date.now() - new Date(iso).getTime()) / 1000);
  if (s < 60) return `${s}s ago`;
  const m = Math.floor(s / 60);
  if (m < 60) return `${m}m ago`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h}h ago`;
  return `${Math.floor(h / 24)}d ago`;
}

function cleanPrompt(p: string) {
  return p.replace(/\[Image:[^\]]*\]/g, '[image]').trim();
}

function Collapsible({ label, defaultOpen = true, children }: {
  label: string;
  defaultOpen?: boolean;
  children: React.ReactNode;
}) {
  const [open, setOpen] = useState(defaultOpen);
  return (
    <div>
      <button
        onClick={() => setOpen(o => !o)}
        style={{
          display: 'flex', alignItems: 'center', gap: 5,
          fontSize: 10, fontWeight: 700, letterSpacing: '0.08em',
          color: '#a09cba', background: 'none', border: 'none',
          cursor: 'pointer', padding: 0, marginBottom: open ? 8 : 0,
          userSelect: 'none',
        }}
        onMouseEnter={e => { (e.currentTarget as HTMLButtonElement).style.color = '#6a67a0'; }}
        onMouseLeave={e => { (e.currentTarget as HTMLButtonElement).style.color = '#a09cba'; }}
      >
        <span style={{ fontSize: 9, width: 8 }}>{open ? '▾' : '▸'}</span>
        {label}
      </button>
      {open && children}
    </div>
  );
}

function Changes({ cp }: { cp: CheckpointData }) {
  const [files, setFiles] = useState<FileDiff[] | null>(null);
  const [loading, setLoading] = useState(false);
  const [open, setOpen] = useState(true);

  const changed = cp.diff_added + cp.diff_modified + cp.diff_removed;

  useEffect(() => {
    if (!open || files !== null) return;
    setLoading(true);
    const params = new URLSearchParams({ to: cp.id });
    if (cp.parent_checkpoint_id) params.set('from', cp.parent_checkpoint_id);
    fetch(`/api/diff?${params}`)
      .then(r => r.json())
      .then((data: FileDiff[]) => { setFiles(data); setLoading(false); })
      .catch(() => setLoading(false));
  }, [open, cp.id, cp.parent_checkpoint_id]);

  // No parent = first checkpoint, no changes = nothing to show
  if (!cp.parent_checkpoint_id) {
    return (
      <div style={{ fontSize: 11, color: '#b0aac8', fontStyle: 'italic' }}>
        initial snapshot · {cp.file_count} files
      </div>
    );
  }

  if (changed === 0) {
    return (
      <div style={{ fontSize: 11, color: '#b0aac8', fontStyle: 'italic' }}>
        no changes
      </div>
    );
  }

  return (
    <div>
      {/* Header row */}
      <div
        onClick={() => setOpen(o => !o)}
        style={{
          display: 'flex', alignItems: 'center', gap: 8,
          padding: '9px 14px',
          background: open ? '#f0edfb' : '#faf9fd',
          border: '1px solid #e2dff0', borderRadius: open ? '7px 7px 0 0' : 7,
          cursor: 'pointer', userSelect: 'none',
        }}
        onMouseEnter={e => { if (!open) (e.currentTarget as HTMLDivElement).style.background = '#f4f1fb'; }}
        onMouseLeave={e => { if (!open) (e.currentTarget as HTMLDivElement).style.background = open ? '#f0edfb' : '#faf9fd'; }}
      >
        <span style={{ color: '#9990c0', fontSize: 10 }}>{open ? '▾' : '▸'}</span>
        <span style={{ color: '#4a4668', fontSize: 12, fontWeight: 600 }}>changes</span>

        <span style={{ marginLeft: 'auto', display: 'flex', gap: 8, fontSize: 11 }}>
          {loading && <span style={{ color: '#c0bcd8', fontSize: 10 }}>loading…</span>}
          {!loading && (
            <>
              {cp.diff_added    > 0 && <span style={{ color: '#1a7f37', fontWeight: 600 }}>+{cp.diff_added}</span>}
              {cp.diff_modified > 0 && <span style={{ color: '#bf8700', fontWeight: 600 }}>~{cp.diff_modified}</span>}
              {cp.diff_removed  > 0 && <span style={{ color: '#cf222e', fontWeight: 600 }}>−{cp.diff_removed}</span>}
              <span style={{ color: '#c0bcd8', fontSize: 10 }}>files</span>
            </>
          )}
        </span>
      </div>

      {open && (
        <div style={{
          padding: '12px 14px', background: '#fff',
          border: '1px solid #e2dff0', borderTop: 'none',
          borderRadius: '0 0 7px 7px',
          display: 'flex', flexDirection: 'column', gap: 8,
        }}>
          {loading && <div style={{ color: '#b0aac8', fontSize: 12 }}>loading diff…</div>}
          {files && files.length === 0 && (
            <div style={{ color: '#b0aac8', fontSize: 12, fontStyle: 'italic' }}>
              no file changes
            </div>
          )}
          {files && files.map(f => (
            <FileDiffView key={f.path} file={f} defaultExpanded={files.length <= 3} />
          ))}
        </div>
      )}
    </div>
  );
}

// Walk up the parent chain to the nearest checkpoint that has a prompt.
// Returns the prompt text and whether it was inherited from an ancestor.
function originatingPrompt(cp: CheckpointData, all: CheckpointData[]): { text: string; from: CheckpointData | null } | null {
  if (cp.prompt && cp.prompt.trim()) return { text: cp.prompt, from: null };
  const byId = new Map(all.map(c => [c.id, c]));
  let cur: CheckpointData | undefined = cp;
  const seen = new Set<string>();
  while (cur?.parent_checkpoint_id && !seen.has(cur.id)) {
    seen.add(cur.id);
    cur = byId.get(cur.parent_checkpoint_id);
    if (cur?.prompt && cur.prompt.trim()) return { text: cur.prompt, from: cur };
  }
  return null;
}

export function Detail({ checkpoint: cp, checkpoints = [] }: { checkpoint: CheckpointData | null; checkpoints?: CheckpointData[] }) {
  if (!cp) {
    return (
      <div style={{
        flex: 1, display: 'flex', alignItems: 'center', justifyContent: 'center',
        color: '#ccc8e0', fontSize: 13, background: '#f8f7fc',
      }}>
        ← select a checkpoint
      </div>
    );
  }

  return (
    <div key={cp.id} style={{
      flex: 1, overflowY: 'auto',
      background: '#f8f7fc',
      padding: '28px 36px',
      display: 'flex', flexDirection: 'column', gap: 22,
    }}>

      {/* Title */}
      <div>
        <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 10 }}>
          <span style={{ fontSize: 10, fontWeight: 700, letterSpacing: '0.06em', color: cp.branch_name === 'main' ? '#7c6ef0' : '#a098c0' }}>
            {cp.branch_name === 'main' ? '◆ CHECKPOINT' : `◆ ${(cp.branch_name || 'branch').toUpperCase()}`}
          </span>
          {cp.is_head && (
            <span style={{ fontSize: 9, fontWeight: 700, color: '#5b44e8', background: '#ede9ff', border: '1px solid #b0a8f0', borderRadius: 3, padding: '1px 6px' }}>HEAD</span>
          )}
          <span style={{ fontSize: 11, color: '#a09cba', marginLeft: 'auto' }}>{timeAgo(cp.created_at)}</span>
        </div>
        <h1 style={{ fontSize: 22, fontWeight: 700, color: '#1a1638', lineHeight: 1.3, wordBreak: 'break-word', fontFamily: 'inherit', letterSpacing: '-0.3px' }}>
          {cp.title || '(untitled)'}
        </h1>
      </div>

      {/* Meta */}
      <div style={{ display: 'grid', gridTemplateColumns: 'auto 1fr', gap: '5px 16px', fontSize: 11, background: '#fff', border: '1px solid #e8e4f4', borderRadius: 8, padding: '12px 16px', alignItems: 'center' }}>
        <span style={{ color: '#a09cba' }}>id</span>
        <span style={{ color: '#5a567a' }}>{cp.id}</span>
        {cp.agent && <><span style={{ color: '#a09cba' }}>agent</span><span style={{ color: '#5a567a' }}>{cp.agent}</span></>}
        {cp.session_id && <><span style={{ color: '#a09cba' }}>session</span><span style={{ color: '#5a567a' }}>{cp.session_id}</span></>}
        {cp.private && <><span style={{ color: '#a09cba' }}>visibility</span><span style={{ color: '#9060c0' }}>private</span></>}
        {cp.parent_checkpoint_id && <><span style={{ color: '#a09cba' }}>parent</span><span style={{ color: '#5a567a' }}>{cp.parent_checkpoint_id}</span></>}
      </div>

      {/* Prompt — falls back to the originating prompt from the nearest ancestor */}
      {(() => {
        const origin = originatingPrompt(cp, checkpoints);
        if (!origin) return null;
        const label = origin.from ? `PROMPT  ·  from ${origin.from.id.slice(0, 8)}` : 'PROMPT';
        return (
          <Collapsible label={label}>
            <pre style={{ fontSize: 12, color: '#4a4668', background: '#fff', border: '1px solid #e8e4f4', borderRadius: 8, padding: '14px 16px', whiteSpace: 'pre-wrap', wordBreak: 'break-word', lineHeight: 1.65, fontFamily: 'inherit', maxHeight: 200, overflowY: 'auto' }}>
              {cleanPrompt(origin.text)}
            </pre>
          </Collapsible>
        );
      })()}

      {/* Response */}
      {cp.response && (
        <Collapsible label="RESPONSE">
          <pre style={{ fontSize: 12, color: '#6a6888', background: '#fff', border: '1px solid #e8e4f4', borderRadius: 8, padding: '14px 16px', whiteSpace: 'pre-wrap', wordBreak: 'break-word', lineHeight: 1.65, fontFamily: 'inherit', maxHeight: 320, overflowY: 'auto' }}>
            {cp.response}
          </pre>
        </Collapsible>
      )}

      {/* Changes */}
      <Changes cp={cp} />

      <div style={{ height: 20 }} />
    </div>
  );
}
