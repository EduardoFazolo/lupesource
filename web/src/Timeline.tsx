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

// ── Branch coloring ───────────────────────────────────────────────────────────

function branchHue(name: string): number {
  if (name === 'main') return 255; // fixed purple for main
  let h = 0;
  for (let i = 0; i < name.length; i++) h = Math.imul(h * 31 + name.charCodeAt(i), 1) | 0;
  // avoid the main purple range (230-270) so branches don't clash
  const raw = Math.abs(h) % 300;
  return raw < 40 ? raw + 300 : raw; // shift hues near purple
}
function branchColor(name: string)    { return `hsl(${branchHue(name)}, 52%, 58%)`; }
function branchColorDim(name: string) { return `hsl(${branchHue(name)}, 40%, 72%)`; }

// ── Helpers ───────────────────────────────────────────────────────────────────

function timeAgo(iso: string) {
  const s = Math.floor((Date.now() - new Date(iso).getTime()) / 1000);
  if (s < 60) return `${s}s`;
  const m = Math.floor(s / 60);
  if (m < 60) return `${m}m`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h}h`;
  return `${Math.floor(h / 24)}d`;
}
function shortId(id: string)             { return id.replace(/-/g, '').slice(0, 7); }
function oneLinePrompt(p: string | null) {
  if (!p) return '';
  return p.replace(/\[Image:[^\]]*\]/g, '[img]').split('\n')[0].slice(0, 72);
}

// ── Lane computation ──────────────────────────────────────────────────────────

const MAIN_X  = 14;  // x-center of main lane dot
const MAIN_W  = 28;  // main lane column width
const LANE_W  = 10;  // px per branch lane
const DOT_Y   = 14;  // y-center of dot from top of row
const DOT_R   =  5;  // dot half-size

interface BranchLane {
  cp: CheckpointData;
  rowIdx: number;
  parentIdx: number;
  lane: number;   // 1-based
  color: string;
}

function computeLanes(checkpoints: CheckpointData[]): {
  rows: CheckpointData[];
  branchLanes: BranchLane[];
  laneOf: Map<string, number>;
  gutterW: number;
} {
  const rows = [...checkpoints].sort(
    (a, b) => new Date(b.created_at).getTime() - new Date(a.created_at).getTime()
  );
  const idToIdx = new Map(rows.map((cp, i) => [cp.id, i]));

  // Collect all distinct non-main branch names, sorted for stable lane assignment
  const branchNames = Array.from(
    new Set(rows.filter(cp => cp.branch_name !== 'main').map(cp => cp.branch_name))
  ).sort();
  const branchToLane = new Map(branchNames.map((name, i) => [name, i + 1]));

  const branchLanes: BranchLane[] = [];
  const laneOf = new Map<string, number>();

  for (let i = 0; i < rows.length; i++) {
    const cp = rows[i];
    if (cp.branch_name === 'main') continue;

    const lane = branchToLane.get(cp.branch_name) ?? 1;
    const parentIdx = cp.parent_checkpoint_id
      ? (idToIdx.get(cp.parent_checkpoint_id) ?? rows.length - 1)
      : rows.length - 1;

    laneOf.set(cp.id, lane);
    branchLanes.push({ cp, rowIdx: i, parentIdx, lane, color: branchColor(cp.branch_name) });
  }

  const maxLane = branchLanes.reduce((m, f) => Math.max(m, f.lane), 0);
  const gutterW = MAIN_W + maxLane * LANE_W;
  return { rows, branchLanes, laneOf, gutterW };
}

// ── Component ─────────────────────────────────────────────────────────────────

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

  const { rows, branchLanes, laneOf, gutterW } = computeLanes(checkpoints);
  const isMain = (cp: CheckpointData) => cp.branch_name === 'main';

  return (
    <div style={{
      width: 310, minWidth: 310,
      height: '100%',
      borderRight: '1px solid #e4e0f0',
      overflowY: 'auto',
      background: '#ffffff',
      display: 'flex',
      flexDirection: 'column',
    }}>
      {rows.map((cp, R) => {
        const isSelected  = selectedId === cp.id;
        const onMain      = isMain(cp);
        const myLane      = onMain ? 0 : (laneOf.get(cp.id) ?? 1);
        const color       = branchColor(cp.branch_name);

        // Branch lane lines passing through this row
        const passing    = branchLanes.filter(f => f.rowIdx <= R && R < f.parentIdx);
        // Branch lanes whose parent is at this row → draw connector
        const connectors = branchLanes.filter(f => f.parentIdx === R);

        const mainAbove = onMain && R > 0 && rows.slice(0, R).some(r => isMain(r));
        const mainBelow = onMain && rows.slice(R + 1).some(r => isMain(r));

        return (
          <div
            key={cp.id}
            ref={cp.is_head ? headRef : undefined}
            onClick={() => onSelect(cp.id)}
            style={{
              display: 'flex',
              alignItems: 'stretch',
              cursor: 'pointer',
              position: 'relative',
              background: isSelected ? C.rowSelected : 'transparent',
              borderLeft: `3px solid ${isSelected ? C.rowSelectedBorder : color}`,
              transition: 'background 0.1s',
            }}
            onMouseEnter={e => { if (!isSelected) (e.currentTarget as HTMLDivElement).style.background = C.rowHover; }}
            onMouseLeave={e => { if (!isSelected) (e.currentTarget as HTMLDivElement).style.background = 'transparent'; }}
          >
            {/* ── Gutter ── */}
            <div style={{ width: gutterW, minWidth: gutterW, flexShrink: 0, position: 'relative', alignSelf: 'stretch' }}>

              {/* Main spine above/below dot */}
              {mainAbove && (
                <div style={{ position: 'absolute', left: MAIN_X - 1, top: 0, height: DOT_Y - DOT_R, width: 2, background: C.line }} />
              )}
              {mainBelow && (
                <div style={{ position: 'absolute', left: MAIN_X - 1, top: DOT_Y + DOT_R, bottom: 0, width: 2, background: C.line }} />
              )}

              {/* Main chain diamond */}
              {onMain && (
                <div style={{
                  position: 'absolute',
                  left: MAIN_X - DOT_R, top: DOT_Y - DOT_R,
                  width: DOT_R * 2, height: DOT_R * 2,
                  background: cp.is_head ? C.dotHead : C.dot,
                  transform: 'rotate(45deg)', borderRadius: 2,
                  boxShadow: cp.is_head ? `0 0 0 3px ${C.headBg}, 0 0 0 4px ${C.dotHead}` : 'none',
                  zIndex: 3,
                }} />
              )}

              {/* Branch lane vertical lines passing through */}
              {passing.map(f => {
                const x = MAIN_W + (f.lane - 1) * LANE_W + Math.floor(LANE_W / 2) - 1;
                const isAtBranch = R === f.rowIdx;
                return (
                  <div key={f.cp.id} style={{
                    position: 'absolute', left: x,
                    top: isAtBranch ? DOT_Y + DOT_R : 0,
                    bottom: 0,
                    width: 2, background: f.color, opacity: 0.7, zIndex: 1,
                  }} />
                );
              })}

              {/* Horizontal connectors at parent rows */}
              {connectors.map(f => {
                const laneX = MAIN_W + (f.lane - 1) * LANE_W + Math.floor(LANE_W / 2);
                return (
                  <div key={`conn-${f.cp.id}`} style={{
                    position: 'absolute', left: MAIN_X, top: DOT_Y - 1,
                    width: laneX - MAIN_X, height: 2,
                    background: f.color, opacity: 0.7, zIndex: 2,
                  }} />
                );
              })}

              {/* Branch dot */}
              {!onMain && (
                <div style={{
                  position: 'absolute',
                  left: MAIN_W + (myLane - 1) * LANE_W + Math.floor(LANE_W / 2) - 4,
                  top: DOT_Y - 4,
                  width: 8, height: 8,
                  borderRadius: '50%',
                  background: color,
                  border: '2px solid white',
                  zIndex: 3,
                }} />
              )}
            </div>

            {/* ── Content ── */}
            <div style={{ flex: 1, minWidth: 0, padding: '10px 12px 8px 4px' }}>

              {/* Meta row */}
              <div style={{ display: 'flex', alignItems: 'center', gap: 5, marginBottom: 3 }}>
                <span style={{ fontSize: 10, color: C.textMuted }}>{shortId(cp.id)}</span>

                {/* Branch pill — shown for non-main branches */}
                {!onMain && (
                  <span style={{
                    fontSize: 8, fontWeight: 600,
                    color: '#fff',
                    background: color,
                    borderRadius: 3, padding: '0 4px', lineHeight: '14px',
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

                <span style={{ marginLeft: 'auto', display: 'flex', alignItems: 'center', gap: 3, minWidth: 0 }}>
                  {cp.session_id && (
                    <span
                      onClick={e => { e.stopPropagation(); navigator.clipboard.writeText(cp.session_id!); }}
                      title="click to copy"
                      style={{
                        fontSize: 7, color: branchColorDim(cp.branch_name),
                        fontFamily: 'monospace', userSelect: 'text',
                        cursor: 'copy', flexShrink: 0,
                        overflow: 'hidden', maxWidth: 80,
                        textOverflow: 'ellipsis', whiteSpace: 'nowrap',
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

              {/* Title */}
              <div style={{
                fontSize: 12, fontWeight: 600, lineHeight: 1.35,
                color: isSelected ? '#2a2060' : C.text,
                wordBreak: 'break-word',
                marginBottom: cp.prompt ? 2 : 0,
              }}>
                {cp.title || '(untitled)'}
              </div>

              {/* Prompt preview */}
              {cp.prompt && (
                <div style={{
                  fontSize: 10, color: C.textSub,
                  overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap',
                  marginBottom: 4,
                }}>
                  {oneLinePrompt(cp.prompt)}
                </div>
              )}

              {/* Diff stats — all branches */}
              {(cp.diff_added + cp.diff_modified + cp.diff_removed) > 0 && (
                <div style={{ display: 'flex', gap: 6, fontSize: 10 }}>
                  {cp.diff_added    > 0 && <span style={{ color: '#1a7f37', fontWeight: 600 }}>+{cp.diff_added}</span>}
                  {cp.diff_modified > 0 && <span style={{ color: '#8a6500', fontWeight: 600 }}>~{cp.diff_modified}</span>}
                  {cp.diff_removed  > 0 && <span style={{ color: '#cf222e', fontWeight: 600 }}>−{cp.diff_removed}</span>}
                  <span style={{ color: C.textMuted }}>files</span>
                </div>
              )}
            </div>

            {/* Branch color triangle — bottom-right corner */}
            <div style={{
              position: 'absolute', bottom: 0, right: 0,
              width: 0, height: 0,
              borderLeft: '14px solid transparent',
              borderBottom: `14px solid ${color}`,
              opacity: 0.45,
              pointerEvents: 'none',
            }} />
          </div>
        );
      })}

      <div style={{ height: 40 }} />
    </div>
  );
}
