import { useEffect, useState } from 'react';
import type { CheckpointData, SaveData } from './types';
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

// fromId: prev save in same checkpoint (seq>0), or parent_save_id (seq=0), or null (first ever)
function SaveDiff({ save, fromId, label }: { save: SaveData; fromId: string | null; label?: string }) {
  const [files, setFiles] = useState<FileDiff[] | null>(null);
  const [loading, setLoading] = useState(false);
  const [open, setOpen] = useState(false);

  useEffect(() => {
    if (!open || files !== null) return;
    setLoading(true);
    const params = new URLSearchParams({ to: save.id });
    if (fromId) params.set('from', fromId);
    fetch(`/api/diff?${params}`)
      .then(r => r.json())
      .then((data: FileDiff[]) => { setFiles(data); setLoading(false); })
      .catch(() => setLoading(false));
  }, [open, save.id, fromId]);

  const added   = files?.filter(f => f.status === 'added').length ?? 0;
  const modified = files?.filter(f => f.status === 'modified').length ?? 0;
  const removed = files?.filter(f => f.status === 'removed').length ?? 0;

  return (
    <div style={{ border: '1px solid #e2dff0', borderRadius: 7, overflow: 'hidden' }}>
      {/* Save header — clickable */}
      <div
        onClick={() => setOpen(o => !o)}
        style={{
          display: 'flex', alignItems: 'center', gap: 10,
          padding: '9px 14px',
          background: open ? '#f0edfb' : '#faf9fd',
          cursor: 'pointer', userSelect: 'none',
        }}
        onMouseEnter={e => { if (!open) (e.currentTarget as HTMLDivElement).style.background = '#f4f1fb'; }}
        onMouseLeave={e => { if (!open) (e.currentTarget as HTMLDivElement).style.background = '#faf9fd'; }}
      >
        <span style={{ color: '#9990c0', fontSize: 10 }}>{open ? '▾' : '▸'}</span>
        <span style={{ color: '#7c6ef0', fontSize: 9 }}>●</span>
        <span style={{ color: '#4a4668', fontWeight: 600, fontSize: 12 }}>
          {label ?? (save.sequence === 0 ? 'initial' : `save ${save.sequence}`)}
        </span>
        {save.message && save.sequence > 0 && (
          <span style={{ fontSize: 11, color: '#9a96b8' }}>{save.message}</span>
        )}

        {/* Right side: diff stats (after load) or snapshot size (before) */}
        <span style={{ marginLeft: 'auto', display: 'flex', alignItems: 'center', gap: 8, fontSize: 11 }}>
          {!files && !loading && (
            <span style={{ color: '#c0bcd8', fontSize: 10 }}>{save.file_count} files in snapshot</span>
          )}
          {loading && <span style={{ color: '#c0bcd8', fontSize: 10 }}>loading…</span>}
          {files && files.length === 0 && (
            <span style={{ color: '#c0bcd8', fontSize: 10 }}>no file changes</span>
          )}
          {files && files.length > 0 && (
            <>
              {added > 0    && <span style={{ color: '#1a7f37', fontWeight: 600 }}>+{added}</span>}
              {modified > 0 && <span style={{ color: '#bf8700', fontWeight: 600 }}>~{modified}</span>}
              {removed > 0  && <span style={{ color: '#cf222e', fontWeight: 600 }}>−{removed}</span>}
            </>
          )}
        </span>
      </div>

      {/* Diff pane */}
      {open && (
        <div style={{ padding: '12px 14px', background: '#fff', borderTop: '1px solid #e2dff0', display: 'flex', flexDirection: 'column', gap: 8 }}>
          {loading && (
            <div style={{ color: '#b0aac8', fontSize: 12 }}>loading diff…</div>
          )}
          {files && files.length === 0 && (
            <div style={{ color: '#b0aac8', fontSize: 12, fontStyle: 'italic' }}>
              No file changes — {fromId ? 'identical to previous checkpoint snapshot' : 'first snapshot, no baseline to compare'}
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

export function Detail({ checkpoint: cp }: { checkpoint: CheckpointData | null }) {
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
    <div style={{
      flex: 1, overflowY: 'auto',
      background: '#f8f7fc',
      padding: '28px 36px',
      display: 'flex', flexDirection: 'column', gap: 22,
    }}>

      {/* Title */}
      <div>
        <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 10 }}>
          <span style={{ fontSize: 10, fontWeight: 700, letterSpacing: '0.06em', color: cp.is_main_chain ? '#7c6ef0' : '#a098c0' }}>
            {cp.is_main_chain ? '◆ CHECKPOINT' : '◆ DEAD BRANCH'}
          </span>
          {cp.is_head && (
            <span style={{ fontSize: 9, fontWeight: 700, color: '#5b44e8', background: '#ede9ff', border: '1px solid #b0a8f0', borderRadius: 3, padding: '1px 6px' }}>
              HEAD
            </span>
          )}
          <span style={{ fontSize: 11, color: '#a09cba', marginLeft: 'auto' }}>{timeAgo(cp.created_at)}</span>
        </div>
        <h1 style={{ fontSize: 22, fontWeight: 700, color: '#1a1638', lineHeight: 1.3, wordBreak: 'break-word', fontFamily: 'inherit', letterSpacing: '-0.3px' }}>
          {cp.title || '(untitled)'}
        </h1>
      </div>

      {/* Meta grid */}
      <div style={{ display: 'grid', gridTemplateColumns: 'auto 1fr', gap: '5px 16px', fontSize: 11, background: '#fff', border: '1px solid #e8e4f4', borderRadius: 8, padding: '12px 16px', alignItems: 'center' }}>
        <span style={{ color: '#a09cba' }}>id</span>
        <span style={{ color: '#5a567a' }}>{cp.id}</span>
        {cp.agent && <><span style={{ color: '#a09cba' }}>agent</span><span style={{ color: '#5a567a' }}>{cp.agent}</span></>}
        {cp.session_id && <><span style={{ color: '#a09cba' }}>session</span><span style={{ color: '#5a567a' }}>{cp.session_id}</span></>}
        {cp.private && <><span style={{ color: '#a09cba' }}>visibility</span><span style={{ color: '#9060c0' }}>private</span></>}
        {cp.parent_save_id && <><span style={{ color: '#a09cba' }}>forked from</span><span style={{ color: '#5a567a' }}>{cp.parent_save_id}</span></>}
      </div>

      {/* Prompt */}
      {cp.prompt && (
        <Collapsible label="PROMPT" defaultOpen={true}>
          <pre style={{ fontSize: 12, color: '#4a4668', background: '#fff', border: '1px solid #e8e4f4', borderRadius: 8, padding: '14px 16px', whiteSpace: 'pre-wrap', wordBreak: 'break-word', lineHeight: 1.65, fontFamily: 'inherit', maxHeight: 200, overflowY: 'auto' }}>
            {cleanPrompt(cp.prompt)}
          </pre>
        </Collapsible>
      )}

      {/* Response */}
      {cp.response && (
        <Collapsible label="RESPONSE" defaultOpen={true}>
          <pre style={{ fontSize: 12, color: '#6a6888', background: '#fff', border: '1px solid #e8e4f4', borderRadius: 8, padding: '14px 16px', whiteSpace: 'pre-wrap', wordBreak: 'break-word', lineHeight: 1.65, fontFamily: 'inherit', maxHeight: 320, overflowY: 'auto' }}>
            {cp.response}
          </pre>
        </Collapsible>
      )}

      {/* Saves with diffs */}
      {cp.saves.length > 0 && (
        <Collapsible label="SAVES" defaultOpen={true}>
          <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
            {cp.saves.map((save, i) => {
              // seq=0: diff against parent checkpoint's last save
              // seq>0: diff against previous save in same checkpoint
              const fromId = i > 0
                ? cp.saves[i - 1].id
                : (cp.parent_save_id ?? null);
              const label = save.sequence === 0
                ? (cp.parent_save_id ? 'snapshot (vs prev checkpoint)' : 'initial snapshot')
                : `save ${save.sequence}`;
              return (
                <SaveDiff key={save.id} save={save} fromId={fromId} label={label} />
              );
            })}
          </div>
        </Collapsible>
      )}

      <div style={{ height: 20 }} />
    </div>
  );
}
