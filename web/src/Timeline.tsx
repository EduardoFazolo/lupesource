import { useEffect, useRef } from 'react';
import type { CheckpointData } from './types';

// ── Colors ───────────────────────────────────────────────────────────────────

const C = {
  line: '#ddd9ee',
  dot: '#7c6ef0',
  dotHead: '#5b44e8',
  headBg: '#ede9ff',
  headText: '#5b44e8',
  headBorder: '#b0a8f0',
  rowHover: '#f2f0fc',
  rowSelected: '#ede9ff',
  rowSelectedBorder: '#7c6ef0',
  text: '#1c1830',
  textSub: '#7a7490',
  textMuted: '#b0aac8',
};

function branchHue(name: string): number {
  if (name === 'main') return 255;
  let h = 0;
  for (let i = 0; i < name.length; i++) h = Math.imul(h * 31 + name.charCodeAt(i), 1) | 0;
  const raw = Math.abs(h) % 300;
  return raw < 40 ? raw + 300 : raw;
}
function branchColor(name: string)    { return `hsl(${branchHue(name)}, 52%, 58%)`; }
function branchColorDim(name: string) { return `hsl(${branchHue(name)}, 40%, 72%)`; }

function timeAgo(iso: string) {
  const s = Math.floor((Date.now() - new Date(iso).getTime()) / 1000);
  if (s < 60) return `${s}s`;
  const m = Math.floor(s / 60);
  if (m < 60) return `${m}m`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h}h`;
  return `${Math.floor(h / 24)}d`;
}
function shortId(id: string) { return id.replace(/-/g, '').slice(0, 7); }
function oneLinePrompt(p: string | null) {
  if (!p) return '';
  return p.replace(/\[Image:[^\]]*\]/g, '[img]').split('\n')[0].slice(0, 72);
}

// ── Tree row model ────────────────────────────────────────────────────────────

type Row =
  | { kind: 'main';   cp: CheckpointData; hasAbove: boolean; hasBelow: boolean;
      // If this main checkpoint is the parent of a branch chain above it:
      branchArmColor: string | null; }
  | { kind: 'branch'; cp: CheckpointData; branchName: string;
      hasAbove: boolean;  // branch spine from top of row to dot
      hasBelow: boolean;  // branch spine from dot to bottom of row
    };

function buildRows(checkpoints: CheckpointData[]): Row[] {
  const main = checkpoints
    .filter(c => c.branch_name === 'main')
    .sort((a, b) => new Date(a.created_at).getTime() - new Date(b.created_at).getTime());

  const byParent = new Map<string, CheckpointData[]>();
  for (const cp of checkpoints) {
    if (cp.parent_checkpoint_id) {
      const list = byParent.get(cp.parent_checkpoint_id) ?? [];
      list.push(cp);
      byParent.set(cp.parent_checkpoint_id, list);
    }
  }

  function chain(entry: CheckpointData): CheckpointData[] {
    const result = [entry];
    let cur = entry;
    for (;;) {
      const next = (byParent.get(cur.id) ?? []).find(c => c.branch_name === entry.branch_name);
      if (!next) break;
      result.push(next);
      cur = next;
    }
    return result;
  }

  const rows: Row[] = [];
  for (let mi = 0; mi < main.length; mi++) {
    const mainCp = main[mi];
    const branchEntries = (byParent.get(mainCp.id) ?? []).filter(c => c.branch_name !== 'main');
    const hasBelow = mi < main.length - 1 || branchEntries.length > 0;
    // branchArmColor: if this row is the parent of a branch, show the arm coming from its dot.
    const branchArmColor = branchEntries.length > 0 ? branchColor(branchEntries[0].branch_name) : null;
    rows.push({ kind: 'main', cp: mainCp, hasAbove: mi > 0, hasBelow, branchArmColor });

    for (const entry of branchEntries) {
      const nodes = chain(entry); // oldest → newest
      // Push oldest first; after rows.reverse(), oldest ends up at TOP (closest to newer main above),
      // newest at BOTTOM (closest to parent main below, which shows the arm).
      for (let ni = 0; ni < nodes.length; ni++) {
        const isOldest = ni === 0;               // will be at TOP after reversal — no spine above
        const isNewest = ni === nodes.length - 1; // will be at BOTTOM after reversal — no spine below (parent arm closes it)
        rows.push({
          kind: 'branch',
          cp: nodes[ni],
          branchName: entry.branch_name,
          hasAbove: !isOldest,  // all except oldest have spine above
          hasBelow: !isNewest,  // all except newest have spine below (parent arm handles the last segment)
        });
      }
    }
  }

  return rows.reverse();
}

// ── Checkpoint card ───────────────────────────────────────────────────────────

interface CardProps {
  cp: CheckpointData;
  selected: boolean;
  onSelect: (id: string) => void;
  headRef: React.RefObject<HTMLDivElement | null>;
  color: string;
  gutter: React.ReactNode;
}

function Card({ cp, selected, onSelect, headRef, color, gutter }: CardProps) {
  return (
    <div
      ref={cp.is_head ? headRef : undefined}
      onClick={() => onSelect(cp.id)}
      style={{
        display: 'flex',
        alignItems: 'stretch',
        cursor: 'pointer',
        background: selected ? C.rowSelected : 'transparent',
        borderLeft: `3px solid ${selected ? C.rowSelectedBorder : color}`,
        transition: 'background 0.1s',
      }}
      onMouseEnter={e => { if (!selected) (e.currentTarget as HTMLDivElement).style.background = C.rowHover; }}
      onMouseLeave={e => { if (!selected) (e.currentTarget as HTMLDivElement).style.background = 'transparent'; }}
    >
      {gutter}

      {/* Content */}
      <div style={{ flex: 1, minWidth: 0, padding: '9px 12px 7px 4px' }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 5, marginBottom: 3 }}>
          <span style={{ fontSize: 10, color: C.textMuted }}>{shortId(cp.id)}</span>

          {cp.branch_name !== 'main' && (
            <span style={{
              fontSize: 8, fontWeight: 600, color: '#fff',
              background: color, borderRadius: 3,
              padding: '0 4px', lineHeight: '14px',
              maxWidth: 80, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap',
            }}>
              {cp.branch_name}
            </span>
          )}

          {cp.is_head && (
            <span style={{
              fontSize: 9, fontWeight: 700,
              color: C.headText, background: C.headBg,
              border: `1px solid ${C.headBorder}`,
              borderRadius: 3, padding: '0 4px', lineHeight: '14px',
            }}>HEAD</span>
          )}

          <span style={{ marginLeft: 'auto', display: 'flex', alignItems: 'center', gap: 3 }}>
            {cp.session_id && (
              <span
                onClick={e => { e.stopPropagation(); navigator.clipboard.writeText(cp.session_id!); }}
                title="click to copy"
                style={{
                  fontSize: 7, color: branchColorDim(cp.branch_name),
                  fontFamily: 'monospace', cursor: 'copy',
                  overflow: 'hidden', maxWidth: 80, textOverflow: 'ellipsis', whiteSpace: 'nowrap',
                }}
              >
                {cp.session_id}
              </span>
            )}
            <span style={{ fontSize: 10, color: C.textMuted, flexShrink: 0 }}>
              {timeAgo(cp.created_at)}
            </span>
          </span>
        </div>

        <div style={{
          fontSize: 12, fontWeight: 600, lineHeight: 1.35,
          color: selected ? '#2a2060' : C.text,
          wordBreak: 'break-word',
          marginBottom: cp.prompt ? 2 : 0,
        }}>
          {cp.title || '(untitled)'}
        </div>

        {cp.prompt && (
          <div style={{
            fontSize: 10, color: C.textSub,
            overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap',
            marginBottom: 4,
          }}>
            {oneLinePrompt(cp.prompt)}
          </div>
        )}

        {(cp.diff_added + cp.diff_modified + cp.diff_removed) > 0 && (
          <div style={{ display: 'flex', gap: 6, fontSize: 10 }}>
            {cp.diff_added    > 0 && <span style={{ color: '#1a7f37', fontWeight: 600 }}>+{cp.diff_added}</span>}
            {cp.diff_modified > 0 && <span style={{ color: '#8a6500', fontWeight: 600 }}>~{cp.diff_modified}</span>}
            {cp.diff_removed  > 0 && <span style={{ color: '#cf222e', fontWeight: 600 }}>−{cp.diff_removed}</span>}
            <span style={{ color: C.textMuted }}>files</span>
          </div>
        )}
      </div>

      {/* Branch color corner */}
      <div style={{
        position: 'absolute', bottom: 0, right: 0,
        width: 0, height: 0,
        borderLeft: '14px solid transparent',
        borderBottom: `14px solid ${color}`,
        opacity: 0.35, pointerEvents: 'none',
      }} />
    </div>
  );
}

// ── Gutter constants ──────────────────────────────────────────────────────────

const MAIN_X   = 14;   // center of main spine
const DOT_Y    = 14;   // dot center y from row top
const DOT_R    =  5;   // dot radius
const BRANCH_X = 38;   // x center of branch spine
const INDENT   = 52;   // total gutter width

// ── Timeline ──────────────────────────────────────────────────────────────────

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

  const rows = buildRows(checkpoints);

  return (
    <div style={{
      width: 320, minWidth: 320,
      height: '100%',
      borderRight: '1px solid #e4e0f0',
      overflowY: 'auto',
      background: '#ffffff',
      display: 'flex',
      flexDirection: 'column',
    }}>
      {rows.map((row) => {
        const isSelected = selectedId === row.cp.id;

        if (row.kind === 'main') {
          const arm = row.branchArmColor;
          const gutter = (
            <div style={{ width: INDENT, minWidth: INDENT, flexShrink: 0, position: 'relative', alignSelf: 'stretch' }}>
              {/* main spine above dot */}
              {row.hasAbove && <div style={{ position: 'absolute', left: MAIN_X - 1, top: 0, height: DOT_Y - DOT_R, width: 2, background: C.line }} />}
              {/* main spine below dot */}
              {row.hasBelow && <div style={{ position: 'absolute', left: MAIN_X - 1, top: DOT_Y + DOT_R, bottom: 0, width: 2, background: C.line }} />}
              {/* branch arm: horizontal from main dot → branch column, + vertical up to row top */}
              {arm && (
                <>
                  <div style={{ position: 'absolute', top: DOT_Y - 1, left: MAIN_X + DOT_R, width: BRANCH_X - MAIN_X - DOT_R, height: 2, background: arm, opacity: 0.8 }} />
                  <div style={{ position: 'absolute', left: BRANCH_X - 1, top: 0, height: DOT_Y, width: 2, background: arm, opacity: 0.6 }} />
                </>
              )}
              {/* diamond */}
              <div style={{
                position: 'absolute',
                left: MAIN_X - DOT_R, top: DOT_Y - DOT_R,
                width: DOT_R * 2, height: DOT_R * 2,
                background: row.cp.is_head ? C.dotHead : C.dot,
                transform: 'rotate(45deg)', borderRadius: 2,
                boxShadow: row.cp.is_head ? `0 0 0 3px ${C.headBg}, 0 0 0 4px ${C.dotHead}` : 'none',
                zIndex: 3,
              }} />
            </div>
          );
          return (
            <div key={row.cp.id} style={{ position: 'relative' }}>
              <Card cp={row.cp} selected={isSelected} onSelect={onSelect} headRef={headRef} color={branchColor('main')} gutter={gutter} />
            </div>
          );
        }

        // branch row
        const bColor = branchColor(row.branchName);
        const gutter = (
          <div style={{ width: INDENT, minWidth: INDENT, flexShrink: 0, position: 'relative', alignSelf: 'stretch' }}>
            {row.hasAbove && <div style={{ position: 'absolute', left: BRANCH_X - 1, top: 0, height: DOT_Y - DOT_R + 1, width: 2, background: bColor, opacity: 0.6 }} />}
            {row.hasBelow && <div style={{ position: 'absolute', left: BRANCH_X - 1, top: DOT_Y + DOT_R - 1, bottom: 0, width: 2, background: bColor, opacity: 0.6 }} />}
            {/* newest node: extend spine to bottom of row to meet parent arm */}
            {!row.hasBelow && <div style={{ position: 'absolute', left: BRANCH_X - 1, top: DOT_Y + DOT_R - 1, bottom: 0, width: 2, background: bColor, opacity: 0.6 }} />}
            <div style={{
              position: 'absolute',
              left: BRANCH_X - DOT_R + 1, top: DOT_Y - DOT_R + 1,
              width: (DOT_R - 1) * 2, height: (DOT_R - 1) * 2,
              borderRadius: '50%', background: bColor,
              border: '2px solid white', zIndex: 3,
            }} />
          </div>
        );
        return (
          <div key={row.cp.id} style={{ position: 'relative' }}>
            <Card cp={row.cp} selected={isSelected} onSelect={onSelect} headRef={headRef} color={bColor} gutter={gutter} />
          </div>
        );
      })}

      <div style={{ height: 40 }} />
    </div>
  );
}
