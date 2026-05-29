import { useEffect, useRef } from 'react';
import type { CheckpointData } from './types';

const C = {
  line: '#ddd9ee',
  lineActive: '#b8b0d8',
  dot: '#7c6ef0',
  dotHead: '#5b44e8',
  dotDead: '#c4bed8',
  deadLine: '#e4dff4',
  deadText: '#a098c0',
  rowHover: '#f2f0fc',
  rowSelected: '#ede9ff',
  rowSelectedBorder: '#7c6ef0',
  text: '#1c1830',
  textSub: '#7a7490',
  textMuted: '#b0aac8',
  savePill: '#ede9f8',
  savePillText: '#7060c0',
  headBg: '#ede9ff',
  headText: '#5b44e8',
  headBorder: '#b0a8f0',
  deadBg: '#f4f2f8',
  deadBorder: '#d8d4ec',
};

function timeAgo(iso: string) {
  const s = Math.floor((Date.now() - new Date(iso).getTime()) / 1000);
  if (s < 60) return `${s}s`;
  const m = Math.floor(s / 60);
  if (m < 60) return `${m}m`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h}h`;
  return `${Math.floor(h / 24)}d`;
}

function shortId(id: string) {
  return id.replace(/-/g, '').slice(0, 7);
}

function oneLinePrompt(p: string | null) {
  if (!p) return '';
  return p.replace(/\[Image:[^\]]*\]/g, '[img]').split('\n')[0].slice(0, 72);
}

interface Props {
  checkpoints: CheckpointData[];
  selectedId: string | null;
  onSelect: (id: string) => void;
}

export function Timeline({ checkpoints, selectedId, onSelect }: Props) {
  const headRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    headRef.current?.scrollIntoView({ block: 'center', behavior: 'instant' });
  }, []);

  const saveToCheckpoint = new Map<string, string>();
  for (const cp of checkpoints) {
    for (const s of cp.saves) saveToCheckpoint.set(s.id, cp.id);
  }

  const mainChain = checkpoints.filter(c => c.is_main_chain);
  const deadByParentCp = new Map<string, CheckpointData[]>();
  for (const cp of checkpoints) {
    if (cp.is_main_chain) continue;
    const parentCpId = cp.parent_save_id ? saveToCheckpoint.get(cp.parent_save_id) : null;
    if (parentCpId) {
      const arr = deadByParentCp.get(parentCpId) ?? [];
      arr.push(cp);
      deadByParentCp.set(parentCpId, arr);
    }
  }

  const TREE_W = 32; // px width of the tree gutter

  return (
    <div style={{
      width: 310,
      minWidth: 310,
      height: '100%',
      borderRight: '1px solid #e4e0f0',
      overflowY: 'auto',
      background: '#ffffff',
      display: 'flex',
      flexDirection: 'column',
    }}>
      {mainChain.map((cp, idx) => {
        const isFirst = idx === 0;
        const isLast = idx === mainChain.length - 1;
        const dead = deadByParentCp.get(cp.id) ?? [];
        const isSelected = selectedId === cp.id;
        const hasDead = dead.length > 0;

        return (
          <div key={cp.id} ref={cp.is_head ? headRef : undefined}>

            {/* ── Main chain row ── */}
            <div
              onClick={() => onSelect(cp.id)}
              style={{
                display: 'flex',
                alignItems: 'stretch',
                cursor: 'pointer',
                background: isSelected ? C.rowSelected : 'transparent',
                borderLeft: `3px solid ${isSelected ? C.rowSelectedBorder : 'transparent'}`,
                transition: 'background 0.1s',
              }}
              onMouseEnter={e => { if (!isSelected) (e.currentTarget as HTMLDivElement).style.background = C.rowHover; }}
              onMouseLeave={e => { if (!isSelected) (e.currentTarget as HTMLDivElement).style.background = 'transparent'; }}
            >
              {/* Tree gutter */}
              <div style={{ width: TREE_W, minWidth: TREE_W, display: 'flex', flexDirection: 'column', alignItems: 'center', flexShrink: 0 }}>
                {/* Line above dot */}
                <div style={{ width: 2, height: 14, background: isFirst ? 'transparent' : C.line, flexShrink: 0 }} />
                {/* Diamond dot */}
                <div style={{
                  width: 10, height: 10, flexShrink: 0,
                  background: cp.is_head ? C.dotHead : C.dot,
                  transform: 'rotate(45deg)',
                  borderRadius: 2,
                  boxShadow: cp.is_head ? `0 0 0 3px ${C.headBg}, 0 0 0 4px ${C.dotHead}` : 'none',
                  zIndex: 1,
                }} />
                {/* Line below dot — extends through saves + dead branches */}
                {(!isLast || hasDead) && (
                  <div style={{ width: 2, flexGrow: 1, minHeight: 8, background: C.line, flexShrink: 0 }} />
                )}
              </div>

              {/* Content */}
              <div style={{ flex: 1, minWidth: 0, padding: '10px 12px 8px 4px' }}>
                <div style={{ display: 'flex', alignItems: 'center', gap: 5, marginBottom: 3 }}>
                  <span style={{ fontSize: 10, color: C.textMuted }}>{shortId(cp.id)}</span>
                  {cp.is_head && (
                    <span style={{
                      fontSize: 9, fontWeight: 700,
                      color: C.headText, background: C.headBg,
                      border: `1px solid ${C.headBorder}`,
                      borderRadius: 3, padding: '0 4px', lineHeight: '14px',
                    }}>HEAD</span>
                  )}
                  <span style={{ fontSize: 10, color: C.textMuted, marginLeft: 'auto' }}>{timeAgo(cp.created_at)}</span>
                </div>

                <div style={{
                  fontSize: 12, fontWeight: 600, lineHeight: 1.35,
                  color: isSelected ? '#2a2060' : C.text,
                  wordBreak: 'break-word', marginBottom: cp.prompt ? 2 : 0,
                }}>
                  {cp.title || '(untitled)'}
                </div>

                {cp.prompt && (
                  <div style={{
                    fontSize: 10, color: C.textSub,
                    overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap',
                  }}>
                    {oneLinePrompt(cp.prompt)}
                  </div>
                )}
              </div>
            </div>

            {/* ── Save rows — nested inside checkpoint ── */}
            {cp.saves.map((s, si) => {
              const isLastSave = si === cp.saves.length - 1;
              const hasMore = !isLastSave || hasDead || !isLast;
              const changed = s.diff_added + s.diff_modified + s.diff_removed;
              const unchanged = changed === 0;

              return (
                <div key={s.id}
                  onClick={() => onSelect(cp.id)}
                  style={{ display: 'flex', alignItems: 'stretch', cursor: 'pointer' }}
                  onMouseEnter={e => { if (!isSelected) (e.currentTarget as HTMLDivElement).style.background = C.rowHover; }}
                  onMouseLeave={e => { (e.currentTarget as HTMLDivElement).style.background = 'transparent'; }}
                >
                  {/* Tree gutter: vertical line + L-connector to save dot */}
                  <div style={{ width: TREE_W, minWidth: TREE_W, flexShrink: 0, position: 'relative' }}>
                    {/* Vertical main line continues */}
                    {hasMore && (
                      <div style={{ position: 'absolute', left: '50%', top: 0, bottom: 0, width: 2, background: C.line, transform: 'translateX(-50%)' }} />
                    )}
                    {/* L-connector: short horizontal to save dot */}
                    <div style={{ position: 'absolute', left: '50%', top: 10, width: 8, height: 2, background: C.line }} />
                    {/* Save dot */}
                    <div style={{
                      position: 'absolute', left: '50%', top: 6,
                      marginLeft: 8,
                      width: 6, height: 6,
                      background: unchanged ? '#ddd8f0' : C.dot,
                      borderRadius: '50%',
                    }} />
                  </div>

                  {/* Save content */}
                  <div style={{
                    flex: 1, minWidth: 0,
                    padding: '4px 12px 4px 16px',
                    display: 'flex', alignItems: 'center', gap: 6,
                  }}>
                    <span style={{ fontSize: 10, color: C.textSub, fontWeight: 500 }}>
                      {s.sequence === 0 ? 'snapshot' : `save ${s.sequence}`}
                    </span>
                    {s.message && s.sequence > 0 && (
                      <span style={{ fontSize: 10, color: C.textMuted, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap', flex: 1 }}>
                        {s.message}
                      </span>
                    )}
                    {/* Diff stats */}
                    <span style={{ marginLeft: 'auto', display: 'flex', gap: 5, fontSize: 10, flexShrink: 0 }}>
                      {unchanged ? (
                        <span style={{ color: C.textMuted, fontStyle: 'italic' }}>no changes</span>
                      ) : (
                        <>
                          {s.diff_added > 0    && <span style={{ color: '#1a7f37', fontWeight: 600 }}>+{s.diff_added}</span>}
                          {s.diff_modified > 0 && <span style={{ color: '#8a6500', fontWeight: 600 }}>~{s.diff_modified}</span>}
                          {s.diff_removed > 0  && <span style={{ color: '#cf222e', fontWeight: 600 }}>−{s.diff_removed}</span>}
                        </>
                      )}
                    </span>
                  </div>
                </div>
              );
            })}

            {/* ── Dead branches ── */}
            {dead.map((d, di) => {
              const isLastDead = di === dead.length - 1;
              const isDeadSelected = selectedId === d.id;

              return (
                <div key={d.id}
                  onClick={() => onSelect(d.id)}
                  style={{
                    display: 'flex',
                    alignItems: 'stretch',
                    cursor: 'pointer',
                    background: isDeadSelected ? '#f5f3fb' : 'transparent',
                    borderLeft: `3px solid ${isDeadSelected ? C.dotDead : 'transparent'}`,
                  }}
                  onMouseEnter={e => { if (!isDeadSelected) (e.currentTarget as HTMLDivElement).style.background = '#faf8ff'; }}
                  onMouseLeave={e => { if (!isDeadSelected) (e.currentTarget as HTMLDivElement).style.background = 'transparent'; }}
                >
                  {/* Tree gutter — fork line */}
                  <div style={{ width: TREE_W, minWidth: TREE_W, display: 'flex', flexDirection: 'column', alignItems: 'center', flexShrink: 0, position: 'relative' }}>
                    {/* Vertical continuation of main line */}
                    {(!isLast || !isLastDead) && (
                      <div style={{ position: 'absolute', left: '50%', top: 0, bottom: 0, width: 2, background: C.deadLine, transform: 'translateX(-50%)' }} />
                    )}
                    {/* Horizontal branch connector */}
                    <div style={{ position: 'absolute', left: '50%', top: 12, right: 0, height: 2, background: C.deadLine }} />
                    {/* Dead dot — offset right */}
                    <div style={{
                      position: 'absolute', right: 2, top: 8,
                      width: 8, height: 8,
                      background: C.dotDead,
                      borderRadius: '50%',
                      border: `2px solid #e0daf0`,
                    }} />
                  </div>

                  {/* Dead branch content */}
                  <div style={{ flex: 1, minWidth: 0, padding: '6px 12px 6px 8px' }}>
                    <div style={{ display: 'flex', alignItems: 'center', gap: 5, marginBottom: 2 }}>
                      <span style={{ fontSize: 9, color: C.deadText, fontWeight: 600 }}>dead</span>
                      <span style={{ fontSize: 9, color: C.textMuted }}>{shortId(d.id)}</span>
                      <span style={{ fontSize: 9, color: C.textMuted, marginLeft: 'auto' }}>{timeAgo(d.created_at)}</span>
                    </div>
                    <div style={{
                      fontSize: 11, color: C.deadText, fontWeight: 500,
                      overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap',
                    }}>
                      {d.title || '(untitled)'}
                    </div>
                    {d.prompt && (
                      <div style={{ fontSize: 9, color: C.textMuted, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap', marginTop: 1 }}>
                        {oneLinePrompt(d.prompt)}
                      </div>
                    )}
                  </div>
                </div>
              );
            })}

          </div>
        );
      })}

      {/* Bottom padding */}
      <div style={{ height: 40 }} />
    </div>
  );
}
