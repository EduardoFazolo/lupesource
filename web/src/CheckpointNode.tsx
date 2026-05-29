import { Handle, Position, type NodeProps } from '@xyflow/react';
import type { CheckpointData } from './types';

function timeAgo(iso: string): string {
  const ms = Date.now() - new Date(iso).getTime();
  const s = Math.floor(ms / 1000);
  if (s < 60) return `${s}s ago`;
  const m = Math.floor(s / 60);
  if (m < 60) return `${m}m ago`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h}h ago`;
  const d = Math.floor(h / 24);
  return `${d}d ago`;
}

function shortId(id: string): string {
  return id.replace(/-/g, '').slice(0, 8);
}

export type CheckpointNodeData = CheckpointData & { isDead?: boolean };

export function CheckpointNode({ data }: NodeProps) {
  const cp = data as unknown as CheckpointNodeData;
  const isDead = cp.isDead ?? false;
  const isHead = cp.is_head;
  const isMain = cp.is_main_chain;

  const borderColor = isDead
    ? '#3a3040'
    : isHead
      ? '#a78bfa'
      : isMain
        ? '#4f46e5'
        : '#2a2a35';

  const bg = isDead ? '#14121a' : isHead ? '#1a1730' : '#16151f';
  const opacity = isDead ? 0.6 : 1;

  return (
    <div
      style={{
        background: bg,
        border: `1px solid ${borderColor}`,
        borderRadius: 10,
        padding: '12px 14px',
        minWidth: 260,
        maxWidth: 320,
        opacity,
        position: 'relative',
        boxShadow: isHead
          ? '0 0 0 2px rgba(167,139,250,0.3), 0 4px 20px rgba(0,0,0,0.5)'
          : '0 2px 12px rgba(0,0,0,0.4)',
      }}
    >
      <Handle
        type="target"
        position={Position.Top}
        style={{ background: borderColor, border: 'none', width: 8, height: 8 }}
      />

      {/* Header row */}
      <div style={{ display: 'flex', alignItems: 'center', gap: 6, marginBottom: 6 }}>
        <span style={{ fontSize: 11, color: isDead ? '#5a4a6a' : '#6366f1', fontWeight: 600 }}>
          {isDead ? '◆ dead' : '◆'}
        </span>
        <span style={{ fontSize: 10, color: '#4a4860', fontFamily: 'monospace' }}>
          {shortId(cp.id)}
        </span>
        <span style={{ fontSize: 10, color: '#3a3850', marginLeft: 'auto' }}>
          {timeAgo(cp.created_at)}
        </span>
        {isHead && (
          <span style={{
            fontSize: 9,
            fontWeight: 700,
            color: '#a78bfa',
            background: 'rgba(167,139,250,0.15)',
            border: '1px solid rgba(167,139,250,0.3)',
            borderRadius: 4,
            padding: '1px 5px',
            letterSpacing: '0.05em',
          }}>
            HEAD
          </span>
        )}
      </div>

      {/* Title */}
      <div style={{
        fontSize: 13,
        fontWeight: 600,
        color: isDead ? '#6a5880' : '#c8c4e0',
        marginBottom: 6,
        lineHeight: 1.35,
        wordBreak: 'break-word',
      }}>
        {cp.title || '(untitled)'}
      </div>

      {/* Prompt */}
      {cp.prompt && (
        <div style={{
          fontSize: 11,
          color: '#5a5870',
          marginBottom: 6,
          lineHeight: 1.4,
          borderLeft: '2px solid #2a2840',
          paddingLeft: 7,
          whiteSpace: 'nowrap',
          overflow: 'hidden',
          textOverflow: 'ellipsis',
          maxWidth: '100%',
        }} title={cp.prompt}>
          {cp.prompt}
        </div>
      )}

      {/* Agent */}
      {cp.agent && (
        <div style={{ fontSize: 10, color: '#44425a', marginBottom: 6 }}>
          <span style={{ color: '#38365a' }}>agent: </span>
          <span style={{ color: '#5a5878' }}>{cp.agent}</span>
        </div>
      )}

      {/* Saves */}
      {cp.saves.length > 0 && (
        <div style={{ marginTop: 8, display: 'flex', flexDirection: 'column', gap: 3 }}>
          {cp.saves.map((s) => (
            <div
              key={s.id}
              style={{
                display: 'flex',
                alignItems: 'center',
                gap: 6,
                fontSize: 10,
                color: isDead ? '#3a3450' : '#4a4868',
                background: isDead ? '#18151f' : '#1c1a28',
                borderRadius: 5,
                padding: '3px 7px',
              }}
            >
              <span style={{ color: isDead ? '#3a3450' : '#5b58a0' }}>●</span>
              <span>{s.sequence === 0 ? 'initial' : `save ${s.sequence}`}</span>
              <span style={{ color: '#3a3858', marginLeft: 'auto' }}>
                {s.file_count} files
              </span>
              {s.message && s.sequence > 0 && (
                <span style={{
                  color: '#3a3858',
                  marginLeft: 4,
                  whiteSpace: 'nowrap',
                  overflow: 'hidden',
                  textOverflow: 'ellipsis',
                  maxWidth: 90,
                }} title={s.message}>
                  {s.message}
                </span>
              )}
            </div>
          ))}
        </div>
      )}

      <Handle
        type="source"
        position={Position.Bottom}
        style={{ background: borderColor, border: 'none', width: 8, height: 8 }}
      />
    </div>
  );
}
