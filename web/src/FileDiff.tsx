import { useState } from 'react';

export interface DiffLine {
  kind: 'context' | 'added' | 'removed';
  content: string;
  old_line: number | null;
  new_line: number | null;
}

export interface DiffHunk {
  old_start: number;
  new_start: number;
  lines: DiffLine[];
}

export interface FileDiff {
  path: string;
  status: 'added' | 'modified' | 'removed';
  is_binary: boolean;
  too_large: boolean;
  hunks: DiffHunk[];
}

const STATUS_COLOR = {
  added:    { bg: '#e6ffec', text: '#1a7f37', dot: '#2da44e' },
  modified: { bg: '#fff8c5', text: '#7d4e00', dot: '#bf8700' },
  removed:  { bg: '#ffebe9', text: '#cf222e', dot: '#cf222e' },
};

const LINE_BG = {
  added:   { row: '#e6ffec', num: '#ccffd8', gutter: '#2da44e' },
  removed: { row: '#ffebe9', num: '#ffd7d5', gutter: '#cf222e' },
  context: { row: 'transparent', num: '#f6f8fa', gutter: 'transparent' },
};

const LINE_SIGN = { added: '+', removed: '-', context: ' ' };

function FileHeader({ file, expanded, onToggle }: {
  file: FileDiff;
  expanded: boolean;
  onToggle: () => void;
}) {
  const { bg, text, dot } = STATUS_COLOR[file.status];
  const parts = file.path.split('/');
  const filename = parts.pop()!;
  const dir = parts.join('/');

  const added = file.hunks.flatMap(h => h.lines).filter(l => l.kind === 'added').length;
  const removed = file.hunks.flatMap(h => h.lines).filter(l => l.kind === 'removed').length;

  return (
    <div
      onClick={onToggle}
      style={{
        display: 'flex', alignItems: 'center', gap: 8,
        padding: '8px 14px',
        background: '#f6f8fa',
        borderBottom: expanded ? '1px solid #d8dce1' : 'none',
        cursor: 'pointer', userSelect: 'none',
        borderRadius: expanded ? '6px 6px 0 0' : 6,
      }}
      onMouseEnter={e => { (e.currentTarget as HTMLDivElement).style.background = '#eef0f3'; }}
      onMouseLeave={e => { (e.currentTarget as HTMLDivElement).style.background = '#f6f8fa'; }}
    >
      {/* Chevron */}
      <span style={{ color: '#6e7781', fontSize: 11, width: 10 }}>
        {expanded ? '▾' : '▸'}
      </span>

      {/* Status dot */}
      <span style={{
        fontSize: 9, fontWeight: 700,
        color: text, background: bg,
        border: `1px solid ${dot}33`,
        borderRadius: 3, padding: '1px 5px',
        lineHeight: '14px',
      }}>
        {file.status.toUpperCase()}
      </span>

      {/* Path */}
      <span style={{ fontSize: 12, color: '#57606a', flex: 1, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
        {dir && <span style={{ color: '#8c959f' }}>{dir}/</span>}
        <span style={{ color: '#1f2328', fontWeight: 600 }}>{filename}</span>
      </span>

      {/* +/- counts */}
      {!file.is_binary && (
        <span style={{ display: 'flex', gap: 5, fontSize: 11, flexShrink: 0 }}>
          {added > 0 && <span style={{ color: '#1a7f37' }}>+{added}</span>}
          {removed > 0 && <span style={{ color: '#cf222e' }}>−{removed}</span>}
        </span>
      )}
    </div>
  );
}

function HunkView({ hunk }: { hunk: DiffHunk }) {
  return (
    <div style={{ fontFamily: 'inherit', fontSize: 12 }}>
      {/* Hunk header */}
      <div style={{
        background: '#ddf4ff',
        color: '#0550ae',
        padding: '3px 10px',
        fontSize: 11,
      }}>
        @@ -{hunk.old_start} +{hunk.new_start} @@
      </div>

      {/* Lines */}
      {hunk.lines.map((line, i) => {
        const s = LINE_BG[line.kind];
        const sign = LINE_SIGN[line.kind];
        return (
          <div key={i} style={{ display: 'flex', background: s.row, minHeight: 20 }}>
            {/* Old line number */}
            <div style={{
              width: 44, minWidth: 44, textAlign: 'right',
              padding: '1px 8px 1px 0',
              color: '#6e7781', fontSize: 11, lineHeight: '18px',
              background: s.num, borderRight: '1px solid #d8dce1',
              userSelect: 'none', flexShrink: 0,
            }}>
              {line.old_line ?? ''}
            </div>
            {/* New line number */}
            <div style={{
              width: 44, minWidth: 44, textAlign: 'right',
              padding: '1px 8px 1px 0',
              color: '#6e7781', fontSize: 11, lineHeight: '18px',
              background: s.num, borderRight: '1px solid #d8dce1',
              userSelect: 'none', flexShrink: 0,
            }}>
              {line.new_line ?? ''}
            </div>
            {/* Sign gutter */}
            <div style={{
              width: 16, minWidth: 16, textAlign: 'center',
              color: line.kind === 'added' ? '#1a7f37' : line.kind === 'removed' ? '#cf222e' : '#8c959f',
              fontSize: 12, lineHeight: '18px',
              userSelect: 'none', flexShrink: 0,
            }}>
              {sign}
            </div>
            {/* Content */}
            <pre style={{
              flex: 1, padding: '1px 6px',
              margin: 0, lineHeight: '18px',
              whiteSpace: 'pre-wrap', wordBreak: 'break-all',
              color: line.kind === 'added' ? '#116329' : line.kind === 'removed' ? '#82071e' : '#1f2328',
              fontFamily: 'inherit', fontSize: 12,
              background: 'transparent',
            }}>
              {line.content}
            </pre>
          </div>
        );
      })}
    </div>
  );
}

export function FileDiffView({ file, defaultExpanded = false }: {
  file: FileDiff;
  defaultExpanded?: boolean;
}) {
  const [expanded, setExpanded] = useState(defaultExpanded);

  return (
    <div style={{
      border: '1px solid #d8dce1',
      borderRadius: 6,
      overflow: 'hidden',
      fontFamily: 'inherit',
    }}>
      <FileHeader file={file} expanded={expanded} onToggle={() => setExpanded(e => !e)} />

      {expanded && (
        <div>
          {file.is_binary ? (
            <div style={{ padding: '12px 14px', color: '#8c959f', fontSize: 12, background: '#fff' }}>
              Binary file not shown
            </div>
          ) : file.too_large ? (
            <div style={{ padding: '12px 14px', color: '#8c959f', fontSize: 12, background: '#fff' }}>
              File too large to display — changed, diff not shown
            </div>
          ) : file.hunks.length === 0 ? (
            <div style={{ padding: '12px 14px', color: '#8c959f', fontSize: 12, background: '#fff' }}>
              {file.status === 'added' ? 'Empty file added' : file.status === 'removed' ? 'File removed' : 'No textual changes'}
            </div>
          ) : (
            <div style={{ background: '#fff', overflowX: 'auto' }}>
              {file.hunks.map((hunk, i) => <HunkView key={i} hunk={hunk} />)}
            </div>
          )}
        </div>
      )}
    </div>
  );
}
