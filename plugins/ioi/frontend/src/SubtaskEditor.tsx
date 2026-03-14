/**
 * Visual subtask definition editor for IOI-style contest problems.
 */
import { useTranslation } from '@broccoli/web-sdk/i18n';
import type { TestCaseSummary } from '@broccoli/web-sdk/problem';
import type React from 'react';
import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from 'react';
import { createPortal } from 'react-dom';

import { useTestCases } from './hooks/useTestCases';

/**
 * Find a portal target inside the Radix Dialog tree.
 * Elements portaled to document.body become inert (Radix Dialog marks siblings
 * as non-interactive). By portaling into the Radix portal container instead,
 * our overlays stay interactive. Falls back to document.body if no dialog found.
 */
function findPortalTarget(el: HTMLElement): HTMLElement {
  const radixPortal = el.closest('[data-radix-portal]') as HTMLElement | null;
  if (radixPortal) return radixPortal;
  const dialog = el.closest('[role="dialog"]') as HTMLElement | null;
  if (dialog?.parentElement) return dialog.parentElement;
  return document.body;
}

/**
 * Compute the offset needed to convert viewport-relative coordinates to
 * coordinates relative to a portal target that may have a CSS transform.
 */
function getPortalOffset(portalTarget: HTMLElement): {
  dx: number;
  dy: number;
} {
  if (portalTarget === document.body) return { dx: 0, dy: 0 };
  // Probe with a fixed element to measure any CSS transform offset on the portal target.
  const probe = document.createElement('div');
  probe.style.cssText =
    'position:fixed;top:0;left:0;width:0;height:0;pointer-events:none';
  portalTarget.appendChild(probe);
  const probeRect = probe.getBoundingClientRect();
  portalTarget.removeChild(probe);
  return { dx: probeRect.left, dy: probeRect.top };
}

const hsl = (n: string, fb: string) => `hsl(var(--${n}, ${fb}))`;

const th = {
  popover: hsl('popover', '0 0% 100%'),
  popoverFg: hsl('popover-foreground', '240 10% 3.9%'),
  card: hsl('card', '0 0% 100%'),
  muted: hsl('muted', '240 4.8% 95.9%'),
  mutedFg: hsl('muted-foreground', '240 3.8% 46.1%'),
  accent: hsl('accent', '240 4.8% 95.9%'),
  border: hsl('border', '240 5.9% 90%'),
  input: hsl('input', '240 5.9% 90%'),
  primary: hsl('primary', '217.2 91.2% 50%'),
  destructive: hsl('destructive', '0 84.2% 60.2%'),
  foreground: hsl('foreground', '240 10% 3.9%'),
  background: hsl('background', '0 0% 100%'),
} as const;

interface SubtaskEditorProps {
  value: unknown;
  schema: { title?: string; description?: string };
  onChange: (value: unknown) => void;
  scope?: { scope: string; problemId?: number; contestId?: number };
}

interface SubtaskValue {
  name: string;
  scoring_method: string;
  max_score: number;
  test_cases: string[];
}

interface EditorStatusMessage {
  tone: 'success' | 'error';
  text: string;
}

/** Canonical string label for a test case: prefer label, fall back to stringified id. */
function tcLabel(tc: TestCaseSummary): string {
  return tc.label || String(tc.id);
}

const SCORING_METHODS = [
  {
    key: 'group_min',
    labelKey: 'ioi.subtask.method.groupMin.label',
    shortKey: 'ioi.subtask.method.groupMin.short',
    hintKey: 'ioi.subtask.method.groupMin.hint',
    color: '#ef4444',
    bg: 'rgba(239,68,68,0.08)',
  },
  {
    key: 'sum',
    labelKey: 'ioi.subtask.method.sum.label',
    shortKey: 'ioi.subtask.method.sum.short',
    hintKey: 'ioi.subtask.method.sum.hint',
    color: '#10b981',
    bg: 'rgba(16,185,129,0.08)',
  },
  {
    key: 'group_mul',
    labelKey: 'ioi.subtask.method.groupMul.label',
    shortKey: 'ioi.subtask.method.groupMul.short',
    hintKey: 'ioi.subtask.method.groupMul.hint',
    color: '#f59e0b',
    bg: 'rgba(245,158,11,0.08)',
  },
] as const;

const SCORING_METHOD_KEYS = new Set(
  SCORING_METHODS.map((method) => method.key),
);

function getMethodInfo(key: string) {
  return SCORING_METHODS.find((m) => m.key === key) ?? SCORING_METHODS[0];
}

function defaultSubtask(
  index: number,
  t: (key: string, params?: Record<string, unknown>) => string,
): SubtaskValue {
  return {
    name: t('ioi.subtask.defaultName', { index: index + 1 }),
    scoring_method: 'group_min',
    max_score: 100,
    test_cases: [],
  };
}

function normalizeImportedSubtasks(value: unknown): SubtaskValue[] | null {
  const rawSubtasks = Array.isArray(value)
    ? value
    : value &&
        typeof value === 'object' &&
        !Array.isArray(value) &&
        Array.isArray((value as { subtasks?: unknown }).subtasks)
      ? (value as { subtasks: unknown[] }).subtasks
      : null;

  if (!rawSubtasks) return null;

  const normalized: SubtaskValue[] = [];
  for (const item of rawSubtasks) {
    if (!item || typeof item !== 'object' || Array.isArray(item)) return null;
    const raw = item as Record<string, unknown>;
    const scoringMethod =
      raw.scoring_method ?? raw.scoringMethod ?? 'group_min';
    if (
      typeof scoringMethod !== 'string' ||
      !SCORING_METHOD_KEYS.has(scoringMethod)
    ) {
      return null;
    }

    const maxScoreRaw = raw.max_score ?? raw.maxScore ?? 100;
    const maxScore =
      typeof maxScoreRaw === 'number'
        ? maxScoreRaw
        : typeof maxScoreRaw === 'string'
          ? maxScoreRaw.trim()
            ? Number(maxScoreRaw)
            : NaN
          : NaN;
    if (!Number.isFinite(maxScore)) return null;

    const rawTestCases = raw.test_cases ?? raw.testCases ?? [];
    if (!Array.isArray(rawTestCases)) return null;
    const testCases = rawTestCases.map((entry) => {
      if (typeof entry === 'string' || typeof entry === 'number') {
        return String(entry);
      }
      return null;
    });
    if (testCases.some((entry) => entry === null)) return null;

    const name = raw.name;
    if (name !== undefined && typeof name !== 'string') return null;

    normalized.push({
      name: typeof name === 'string' ? name : '',
      scoring_method: scoringMethod,
      max_score: maxScore,
      test_cases: testCases as string[],
    });
  }

  return normalized;
}

const MAX_RANGE_SPAN = 10000;

/** Parse "1-5, 8, 10-12" into a sorted array of unique positions. */
function parseRanges(input: string): number[] {
  const result = new Set<number>();
  for (const part of input.split(/[,;\s]+/)) {
    const trimmed = part.trim();
    if (!trimmed) continue;
    const rangeMatch = trimmed.match(/^(\d+)\s*-\s*(\d+)$/);
    if (rangeMatch) {
      const lo = parseInt(rangeMatch[1]);
      const hi = parseInt(rangeMatch[2]);
      if (!isNaN(lo) && !isNaN(hi) && Math.abs(hi - lo) <= MAX_RANGE_SPAN) {
        for (let i = Math.min(lo, hi); i <= Math.max(lo, hi); i++)
          result.add(i);
      }
    } else {
      const n = parseInt(trimmed);
      if (!isNaN(n)) result.add(n);
    }
  }
  return [...result].sort((a, b) => a - b);
}

const mono: React.CSSProperties = {
  fontFamily:
    '"JetBrains Mono", ui-monospace, "Cascadia Code", Menlo, monospace',
  fontVariantNumeric: 'tabular-nums',
};

const fieldLabel: React.CSSProperties = {
  fontSize: '9px',
  fontWeight: 700,
  textTransform: 'uppercase',
  letterSpacing: '0.08em',
  opacity: 0.4,
  display: 'block',
  marginBottom: '4px',
};

const fieldInput: React.CSSProperties = {
  padding: '6px 10px',
  borderRadius: '6px',
  border: `1px solid ${th.input}`,
  background: th.card,
  color: 'inherit',
  fontSize: '13px',
  outline: 'none',
  boxSizing: 'border-box' as const,
  transition: 'border-color 0.15s, box-shadow 0.15s',
};

const headerActionButton: React.CSSProperties = {
  background: 'none',
  border: `1px solid ${th.border}`,
  borderRadius: '6px',
  padding: '6px 10px',
  cursor: 'pointer',
  fontSize: '11px',
  fontWeight: 600,
  color: 'inherit',
  opacity: 0.75,
  transition: 'opacity 0.15s, border-color 0.15s, color 0.15s',
};

function PreviewPopover({
  tc,
  anchorRect,
  onMouseEnter,
  onMouseLeave,
  portalTarget,
  portalOffset,
}: {
  tc: TestCaseListItem;
  anchorRect: DOMRect | null;
  onMouseEnter?: () => void;
  onMouseLeave?: () => void;
  portalTarget: HTMLElement;
  portalOffset: { dx: number; dy: number };
}) {
  const { t } = useTranslation();
  if (!anchorRect) return null;
  // Estimate height for flip decision; actual height auto-sizes
  const estHeight = 200;
  const flipAbove = anchorRect.bottom + estHeight + 8 > window.innerHeight;
  const left = Math.max(
    8,
    Math.min(
      anchorRect.left + anchorRect.width / 2 - 160,
      window.innerWidth - 328,
    ),
  );

  // Bridge wrapper: the outer div extends from the anchor edge (no gap) so
  // the mouse can transition from chip -> popover without leaving a tracked element.
  return createPortal(
    <div
      onMouseEnter={onMouseEnter}
      onMouseLeave={onMouseLeave}
      style={{
        position: 'fixed',
        ...(flipAbove
          ? { bottom: window.innerHeight - anchorRect.top - portalOffset.dy }
          : { top: anchorRect.bottom - portalOffset.dy }),
        left: left - portalOffset.dx,
        width: '320px',
        zIndex: 100000,
        pointerEvents: 'auto',
      }}
    >
      <div
        style={{
          ...(flipAbove ? { marginBottom: '4px' } : { marginTop: '4px' }),
          background: th.popover,
          color: th.popoverFg,
          backdropFilter: 'blur(8px)',
          border: `1px solid ${th.border}`,
          borderRadius: '10px',
          boxShadow:
            '0 12px 40px rgba(0,0,0,0.18), 0 4px 12px rgba(0,0,0,0.08)',
          padding: '12px',
          isolation: 'isolate',
        }}
      >
        <div
          style={{
            display: 'flex',
            alignItems: 'center',
            gap: '8px',
            marginBottom: '8px',
          }}
        >
          <span
            style={{ ...mono, fontSize: '12px', fontWeight: 700, opacity: 0.6 }}
          >
            #{tc.position + 1}
          </span>
          {tc.label && (
            <span style={{ ...mono, fontSize: '11px', opacity: 0.5 }}>
              {tc.label}
            </span>
          )}
          {tc.is_sample && (
            <span
              style={{
                fontSize: '9px',
                fontWeight: 700,
                textTransform: 'uppercase',
                letterSpacing: '0.05em',
                padding: '1px 6px',
                borderRadius: '4px',
                background: 'rgba(99,102,241,0.1)',
                color: '#6366f1',
              }}
            >
              {t('ioi.subtask.preview.sample')}
            </span>
          )}
          <span
            style={{
              ...mono,
              fontSize: '11px',
              opacity: 0.5,
              marginLeft: 'auto',
            }}
          >
            {tc.score} pts
          </span>
        </div>
        {tc.description && (
          <div
            style={{
              fontSize: '11px',
              opacity: 0.6,
              marginBottom: '8px',
              lineHeight: 1.4,
            }}
          >
            {tc.description.length > 200
              ? tc.description.slice(0, 200) + '…'
              : tc.description}
          </div>
        )}
        <div
          style={{
            display: 'grid',
            gridTemplateColumns: '1fr 1fr',
            gap: '8px',
          }}
        >
          <div>
            <div style={{ ...fieldLabel, marginBottom: '2px' }}>
              {t('ioi.subtask.preview.input')}
            </div>
            <pre
              style={{
                ...mono,
                fontSize: '10px',
                lineHeight: 1.4,
                margin: 0,
                padding: '6px',
                borderRadius: '4px',
                maxHeight: '80px',
                overflow: 'auto',
                overscrollBehavior: 'contain',
                background: th.muted,
                whiteSpace: 'pre-wrap',
                wordBreak: 'break-all',
              }}
            >
              {tc.input_preview || t('ioi.subtask.preview.empty')}
            </pre>
          </div>
          <div>
            <div style={{ ...fieldLabel, marginBottom: '2px' }}>
              {t('ioi.subtask.preview.output')}
            </div>
            <pre
              style={{
                ...mono,
                fontSize: '10px',
                lineHeight: 1.4,
                margin: 0,
                padding: '6px',
                borderRadius: '4px',
                maxHeight: '80px',
                overflow: 'auto',
                overscrollBehavior: 'contain',
                background: th.muted,
                whiteSpace: 'pre-wrap',
                wordBreak: 'break-all',
              }}
            >
              {tc.output_preview || t('ioi.subtask.preview.empty')}
            </pre>
          </div>
        </div>
      </div>
    </div>,
    portalTarget,
  );
}

/** Shared hover-with-delay logic for components that show PreviewPopover. */
function useHoverPopover() {
  const [hoverRect, setHoverRect] = useState<DOMRect | null>(null);
  const closeTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const anchorRef = useRef<HTMLElement | null>(null);

  const clearClose = () => {
    if (closeTimer.current) {
      clearTimeout(closeTimer.current);
      closeTimer.current = null;
    }
  };
  const scheduleClose = () => {
    clearClose();
    closeTimer.current = setTimeout(() => setHoverRect(null), 300);
  };

  useEffect(() => () => clearClose(), []);

  const anchorHandlers = {
    onMouseEnter: () => {
      clearClose();
      if (anchorRef.current)
        setHoverRect(anchorRef.current.getBoundingClientRect());
    },
    onMouseLeave: scheduleClose,
  };
  const popoverHandlers = {
    onMouseEnter: clearClose,
    onMouseLeave: scheduleClose,
  };

  return { hoverRect, anchorRef, anchorHandlers, popoverHandlers };
}

function TcChip({
  tc,
  onRemove,
  onDragStart,
  isDuplicate,
  portalTarget,
  portalOffset,
}: {
  tc: TestCaseListItem;
  onRemove: () => void;
  onDragStart?: (e: React.DragEvent) => void;
  isDuplicate?: boolean;
  portalTarget: HTMLElement;
  portalOffset: { dx: number; dy: number };
}) {
  const { hoverRect, anchorRef, anchorHandlers, popoverHandlers } =
    useHoverPopover();

  const dupBorder = isDuplicate
    ? '1.5px dashed rgba(239,68,68,0.5)'
    : undefined;
  const dupBg = isDuplicate ? 'rgba(239,68,68,0.06)' : undefined;

  return (
    <>
      <span
        ref={anchorRef as React.RefObject<HTMLSpanElement>}
        draggable={!!onDragStart}
        onDragStart={onDragStart}
        onMouseEnter={anchorHandlers.onMouseEnter}
        onMouseLeave={anchorHandlers.onMouseLeave}
        style={{
          display: 'inline-flex',
          alignItems: 'center',
          gap: '4px',
          padding: '3px 8px',
          borderRadius: '6px',
          background: dupBg ?? (hoverRect ? th.accent : th.card),
          border:
            dupBorder ?? `1px solid ${hoverRect ? th.primary : th.border}`,
          fontSize: '11px',
          ...mono,
          cursor: onDragStart ? 'grab' : 'default',
          transition: 'border-color 0.15s, box-shadow 0.15s, background 0.15s',
          userSelect: 'none',
          boxShadow: hoverRect
            ? `0 0 0 1px ${isDuplicate ? 'rgba(239,68,68,0.4)' : th.primary}`
            : '0 1px 0 0 rgba(0,0,0,0.06)',
        }}
      >
        {tc.label ? (
          <>
            <span style={{ fontSize: '11px' }}>{tc.label}</span>
            <span style={{ opacity: 0.35, fontSize: '9px' }}>
              #{tc.position + 1}
            </span>
          </>
        ) : (
          <>
            <span style={{ opacity: 0.5, fontSize: '9px' }}>#</span>
            {tc.position + 1}
          </>
        )}
        {tc.is_sample && (
          <span
            style={{
              width: '4px',
              height: '4px',
              borderRadius: '50%',
              background: '#6366f1',
              flexShrink: 0,
            }}
          />
        )}
        {isDuplicate && (
          <span
            style={{
              width: '6px',
              height: '6px',
              borderRadius: '50%',
              flexShrink: 0,
              background: '#ef4444',
              boxShadow: '0 0 0 2px rgba(239,68,68,0.2)',
            }}
          />
        )}
        <button
          type="button"
          onClick={(e) => {
            e.stopPropagation();
            onRemove();
          }}
          style={{
            background: 'none',
            border: 'none',
            cursor: 'pointer',
            fontSize: '10px',
            opacity: 0.3,
            padding: '0 1px',
            color: 'inherit',
            lineHeight: 1,
            transition: 'opacity 0.15s',
          }}
          onMouseEnter={(e) => {
            e.currentTarget.style.opacity = '0.8';
          }}
          onMouseLeave={(e) => {
            e.currentTarget.style.opacity = '0.3';
          }}
        >
          ✕
        </button>
      </span>
      {hoverRect && (
        <PreviewPopover
          tc={tc}
          anchorRect={hoverRect}
          onMouseEnter={popoverHandlers.onMouseEnter}
          onMouseLeave={popoverHandlers.onMouseLeave}
          portalTarget={portalTarget}
          portalOffset={portalOffset}
        />
      )}
    </>
  );
}

function PoolCard({
  tc,
  state,
  ownerLabel,
  onClick,
  onDragStart,
  isDuplicate,
  portalTarget,
  portalOffset,
}: {
  tc: TestCaseListItem;
  state: 'unassigned' | 'this' | 'other';
  ownerLabel?: string;
  onClick: (e: React.MouseEvent) => void;
  onDragStart: (e: React.DragEvent) => void;
  isDuplicate?: boolean;
  portalTarget: HTMLElement;
  portalOffset: { dx: number; dy: number };
}) {
  const { hoverRect, anchorRef, anchorHandlers, popoverHandlers } =
    useHoverPopover();

  const borderColor = isDuplicate
    ? 'rgba(239,68,68,0.5)'
    : state === 'this'
      ? 'rgba(16,185,129,0.5)'
      : th.border;

  const bg = isDuplicate
    ? 'rgba(239,68,68,0.04)'
    : state === 'this'
      ? 'rgba(16,185,129,0.04)'
      : state === 'other'
        ? th.muted
        : th.card;

  return (
    <>
      <div
        ref={anchorRef as React.RefObject<HTMLDivElement>}
        draggable
        onDragStart={onDragStart}
        onClick={onClick}
        onMouseEnter={anchorHandlers.onMouseEnter}
        onMouseLeave={anchorHandlers.onMouseLeave}
        style={{
          padding: '8px 10px',
          borderRadius: '8px',
          border: `1.5px solid ${borderColor}`,
          background: bg,
          cursor: state === 'other' ? 'not-allowed' : 'pointer',
          opacity: state === 'other' ? 0.45 : 1,
          transition: 'all 0.15s',
          userSelect: 'none',
          position: 'relative',
          minWidth: 0,
        }}
      >
        <div style={{ display: 'flex', alignItems: 'center', gap: '6px' }}>
          {tc.label ? (
            <>
              <span
                style={{
                  ...mono,
                  fontSize: '11px',
                  fontWeight: 700,
                  opacity: 0.8,
                  overflow: 'hidden',
                  textOverflow: 'ellipsis',
                  whiteSpace: 'nowrap',
                }}
              >
                {tc.label}
              </span>
              <span style={{ ...mono, fontSize: '9px', opacity: 0.35 }}>
                #{tc.position + 1}
              </span>
            </>
          ) : (
            <span
              style={{
                ...mono,
                fontSize: '12px',
                fontWeight: 700,
                opacity: 0.7,
              }}
            >
              {tc.position + 1}
            </span>
          )}
          {tc.is_sample && (
            <span
              style={{
                fontSize: '8px',
                fontWeight: 700,
                textTransform: 'uppercase',
                letterSpacing: '0.04em',
                padding: '1px 5px',
                borderRadius: '3px',
                background: 'rgba(99,102,241,0.1)',
                color: '#6366f1',
              }}
            >
              S
            </span>
          )}
          <span
            style={{
              ...mono,
              fontSize: '10px',
              opacity: 0.4,
              marginLeft: 'auto',
            }}
          >
            {tc.score}
          </span>
        </div>
        {tc.description && (
          <div
            style={{
              fontSize: '10px',
              opacity: 0.5,
              marginTop: '3px',
              whiteSpace: 'nowrap',
              overflow: 'hidden',
              textOverflow: 'ellipsis',
            }}
          >
            {tc.description}
          </div>
        )}
        {state === 'this' && (
          <div
            style={{
              position: 'absolute',
              top: '3px',
              right: '3px',
              width: '6px',
              height: '6px',
              borderRadius: '50%',
              background: '#10b981',
            }}
          />
        )}
        {state === 'other' && ownerLabel && (
          <div
            style={{
              fontSize: '9px',
              opacity: 0.6,
              marginTop: '3px',
              fontStyle: 'italic',
            }}
          >
            → {ownerLabel}
          </div>
        )}
      </div>
      {hoverRect && (
        <PreviewPopover
          tc={tc}
          anchorRect={hoverRect}
          onMouseEnter={popoverHandlers.onMouseEnter}
          onMouseLeave={popoverHandlers.onMouseLeave}
          portalTarget={portalTarget}
          portalOffset={portalOffset}
        />
      )}
    </>
  );
}

// Position mode requires explicit # prefix to avoid confusion with numeric labels.
// Examples: #1-10, #15, #1-5,8,10-12
const POS_RANGE_RE = /^#[\d\s,;-]+$/;

function UnifiedSearch({
  testCases,
  subtask,
  subtaskIdx,
  updateSubtask,
  tcByPosition,
  assignmentMap,
  duplicateSet,
  subtaskNames,
  portalTarget,
  portalOffset,
}: {
  testCases: TestCaseListItem[];
  subtask: SubtaskValue;
  subtaskIdx: number;
  updateSubtask: (index: number, patch: Partial<SubtaskValue>) => void;
  tcByPosition: Map<number, TestCaseListItem>;
  assignmentMap: Map<string, number[]>;
  duplicateSet: Set<string>;
  subtaskNames: string[];
  portalTarget: HTMLElement;
  portalOffset: { dx: number; dy: number };
}) {
  const { t } = useTranslation();
  const [query, setQuery] = useState('');
  const [showDropdown, setShowDropdown] = useState(false);
  const [activeIndex, setActiveIndex] = useState(-1);
  const [inputRect, setInputRect] = useState<{
    top: number;
    bottom: number;
    left: number;
    width: number;
  } | null>(null);
  const blurTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const listboxRef = useRef<HTMLDivElement>(null);
  const listboxId = `unified-search-${subtaskIdx}`;

  const cancelBlur = useCallback(() => {
    if (blurTimeoutRef.current) {
      clearTimeout(blurTimeoutRef.current);
      blurTimeoutRef.current = null;
    }
  }, []);

  useEffect(() => () => cancelBlur(), [cancelBlur]);

  // Reposition dropdown on scroll/resize
  const measureInput = useCallback(() => {
    if (!inputRef.current) return;
    const r = inputRef.current.getBoundingClientRect();
    if (r.bottom < 0 || r.top > window.innerHeight) {
      setShowDropdown(false);
      return;
    }
    setInputRect({
      top: r.top,
      bottom: r.bottom,
      left: r.left,
      width: r.width,
    });
  }, []);

  useEffect(() => {
    if (!showDropdown) return;
    measureInput();
    window.addEventListener('scroll', measureInput, true);
    window.addEventListener('resize', measureInput);
    return () => {
      window.removeEventListener('scroll', measureInput, true);
      window.removeEventListener('resize', measureInput);
    };
  }, [showDropdown, measureInput]);

  const q = query.trim();
  const isPositionMode = q.length > 0 && POS_RANGE_RE.test(q);

  const posResults = useMemo(() => {
    if (!isPositionMode || !q) return null;
    const rangeStr = q.slice(1);
    const positions = parseRanges(rangeStr);
    const resolved: TestCaseListItem[] = [];
    const invalid: number[] = [];
    for (const pos of positions) {
      const tc = tcByPosition.get(pos);
      if (tc) resolved.push(tc);
      else invalid.push(pos);
    }
    return { resolved, invalid };
  }, [isPositionMode, q, tcByPosition]);

  const labelMatches = useMemo(() => {
    if (isPositionMode || !q) return [];
    let re: RegExp | null = null;
    try {
      re = new RegExp(q, 'i');
    } catch {
      /* invalid regex */
    }
    return testCases.filter((tc) => {
      const l = tcLabel(tc);
      return re ? re.test(l) : l.toLowerCase().includes(q.toLowerCase());
    });
  }, [q, testCases, isPositionMode]);

  // Build dropdown items — only show addable (non-added, non-duplicate) items
  const items = useMemo(() => {
    const result: Array<{
      type: 'batch' | 'item';
      tc?: TestCaseListItem;
      labels?: string[];
      count?: number;
    }> = [];
    const source = isPositionMode ? (posResults?.resolved ?? []) : labelMatches;
    // Filter to only addable items: not already in this subtask
    const addable = source.filter(
      (tc) => !subtask.test_cases.includes(tcLabel(tc)),
    );
    if (addable.length > 1) {
      result.push({
        type: 'batch',
        labels: addable.map((tc) => tcLabel(tc)),
        count: addable.length,
      });
    }
    for (const tc of addable.slice(0, 50)) {
      result.push({ type: 'item', tc });
    }
    return { items: result, addableTotal: addable.length };
  }, [isPositionMode, posResults, labelMatches, subtask.test_cases]);

  const addLabels = useCallback(
    (labels: string[]) => {
      const existing = new Set(subtask.test_cases);
      const toAdd = labels.filter((l) => !existing.has(l));
      if (toAdd.length > 0)
        updateSubtask(subtaskIdx, {
          test_cases: [...subtask.test_cases, ...toAdd],
        });
      setQuery('');
      setShowDropdown(false);
      setActiveIndex(-1);
    },
    [subtask, subtaskIdx, updateSubtask],
  );

  const scrollActiveIntoView = useCallback(
    (index: number) => {
      const el = listboxRef.current?.querySelector(
        `[id="${listboxId}-opt-${index}"]`,
      ) as HTMLElement | null;
      if (el) el.scrollIntoView({ block: 'nearest' });
    },
    [listboxId],
  );

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      const hasItems = items.items.length > 0;
      const isOpen = showDropdown && q.length > 0 && hasItems;

      if (e.key === 'ArrowDown') {
        e.preventDefault();
        if (!isOpen) {
          if (q) setShowDropdown(true);
          return;
        }
        setActiveIndex((prev) => {
          const n = prev < items.items.length - 1 ? prev + 1 : 0;
          requestAnimationFrame(() => scrollActiveIntoView(n));
          return n;
        });
      } else if (e.key === 'ArrowUp') {
        e.preventDefault();
        if (!isOpen) return;
        setActiveIndex((prev) => {
          const n = prev > 0 ? prev - 1 : items.items.length - 1;
          requestAnimationFrame(() => scrollActiveIntoView(n));
          return n;
        });
      } else if (e.key === 'Enter') {
        e.preventDefault();
        if (isOpen && activeIndex >= 0 && activeIndex < items.items.length) {
          const item = items.items[activeIndex];
          if (item.type === 'batch') addLabels(item.labels!);
          else if (item.tc) addLabels([tcLabel(item.tc)]);
        } else if (isPositionMode && posResults) {
          const labels = posResults.resolved.map((tc) => tcLabel(tc));
          addLabels(labels);
        } else if (!isPositionMode && labelMatches.length === 1) {
          addLabels([tcLabel(labelMatches[0])]);
        } else if (!isPositionMode && labelMatches.length > 1) {
          const exact = labelMatches.find(
            (tc) => tcLabel(tc).toLowerCase() === q.toLowerCase(),
          );
          if (exact) addLabels([tcLabel(exact)]);
        }
      } else if (e.key === 'Escape') {
        setShowDropdown(false);
        setActiveIndex(-1);
        inputRef.current?.blur();
      } else if (e.key === 'Tab') {
        setShowDropdown(false);
        setActiveIndex(-1);
      }
    },
    [
      showDropdown,
      q,
      items,
      activeIndex,
      isPositionMode,
      posResults,
      labelMatches,
      addLabels,
      scrollActiveIntoView,
    ],
  );

  const isOpen = showDropdown && q.length > 0;
  const hasResults = items.items.length > 0;
  const activeDescendant =
    isOpen && hasResults && activeIndex >= 0
      ? `${listboxId}-opt-${activeIndex}`
      : undefined;

  // Dropdown positioning. Portal into dialog tree so Radix doesn't block events
  const MAX_DD = 280;
  const spaceBelow = inputRect ? window.innerHeight - inputRect.bottom : MAX_DD;
  const spaceAbove = inputRect?.top ?? MAX_DD;
  const flipAbove = spaceBelow < 150 && spaceAbove > spaceBelow;
  const ddTop = flipAbove
    ? undefined
    : (inputRect?.bottom ?? 0) - portalOffset.dy + 2;
  const ddBottom = flipAbove
    ? window.innerHeight - (inputRect?.top ?? 0) - portalOffset.dy + 2
    : undefined;
  const ddMaxH = Math.min(MAX_DD, (flipAbove ? spaceAbove : spaceBelow) - 8);

  const renderDropdown = () => {
    if (!isOpen || !inputRect) return null;

    const dropdownStyle: React.CSSProperties = {
      position: 'fixed',
      top: ddTop,
      bottom: ddBottom,
      left: inputRect.left - portalOffset.dx,
      width: inputRect.width,
      zIndex: 100000,
      background: th.popover,
      color: th.popoverFg,
      border: `1px solid ${th.border}`,
      borderRadius: '8px',
      boxShadow: '0 8px 24px rgba(0,0,0,0.15), 0 2px 6px rgba(0,0,0,0.08)',
      maxHeight: ddMaxH,
      overflowY: 'auto',
      overscrollBehavior: 'contain',
      boxSizing: 'border-box',
    };

    if (!hasResults) {
      const noMatchMsg = isPositionMode
        ? t('ioi.subtask.noMatchPosition')
        : t('ioi.subtask.noMatchLabel');
      return createPortal(
        <div
          style={{ ...dropdownStyle, padding: '8px 10px' }}
          onMouseDown={(e) => {
            e.preventDefault();
            cancelBlur();
          }}
        >
          <span style={{ fontSize: '11px', opacity: 0.5, fontStyle: 'italic' }}>
            {noMatchMsg}
          </span>
        </div>,
        portalTarget,
      );
    }

    return createPortal(
      <div
        ref={listboxRef}
        id={listboxId}
        role="listbox"
        aria-label="Search results"
        style={dropdownStyle}
        onMouseDown={(e) => {
          e.preventDefault();
          cancelBlur();
        }}
      >
        {/* Mode badge */}
        <div
          style={{
            padding: '4px 10px',
            fontSize: '9px',
            fontWeight: 700,
            textTransform: 'uppercase',
            letterSpacing: '0.05em',
            opacity: 0.4,
            borderBottom: `1px solid ${th.border}`,
          }}
        >
          {isPositionMode
            ? t('ioi.subtask.positions')
            : t('ioi.subtask.labels')}
          {isPositionMode && posResults?.invalid.length ? (
            <span
              style={{
                color: '#dc2626',
                marginLeft: '8px',
                fontWeight: 600,
                opacity: 1,
              }}
            >
              {posResults.invalid.length > 3
                ? `${posResults.invalid.length} ${t('ioi.subtask.notFound')}`
                : `#${posResults.invalid.join(', #')} ${t('ioi.subtask.notFound')}`}
            </span>
          ) : null}
        </div>

        {items.items.map((item, idx) => {
          const isActive = activeIndex === idx;
          if (item.type === 'batch') {
            return (
              <div
                key="batch"
                id={`${listboxId}-opt-${idx}`}
                role="option"
                aria-selected={isActive}
                onClick={() => addLabels(item.labels!)}
                onMouseEnter={() => setActiveIndex(idx)}
                style={{
                  padding: '6px 10px',
                  cursor: 'pointer',
                  fontSize: '10px',
                  fontWeight: 700,
                  color: '#4f46e5',
                  textTransform: 'uppercase',
                  letterSpacing: '0.03em',
                  background: isActive
                    ? 'rgba(79,70,229,0.12)'
                    : 'rgba(79,70,229,0.04)',
                  borderBottom: `1px solid ${th.border}`,
                  boxSizing: 'border-box',
                }}
              >
                {t('ioi.subtask.addCount', {
                  count: item.count,
                  mode: isPositionMode
                    ? t('ioi.subtask.positions').toLowerCase()
                    : 'matches',
                })}
              </div>
            );
          }
          const tc = item.tc!;
          const label = tcLabel(tc);
          const owners = assignmentMap.get(label) ?? [];
          const otherOwner =
            owners.length > 0 && !owners.includes(subtaskIdx)
              ? owners[0]
              : null;
          const isDup = duplicateSet.has(label);
          return (
            <div
              key={tc.id}
              id={`${listboxId}-opt-${idx}`}
              role="option"
              aria-selected={isActive}
              onClick={() => addLabels([label])}
              onMouseEnter={() => setActiveIndex(idx)}
              style={{
                padding: '5px 10px',
                fontSize: '11px',
                display: 'flex',
                alignItems: 'center',
                gap: '8px',
                background: isActive ? 'rgba(79,70,229,0.08)' : 'transparent',
                borderBottom: `1px solid ${th.border}`,
                cursor: 'pointer',
                boxSizing: 'border-box',
              }}
            >
              <span style={{ ...mono, fontWeight: 600 }}>
                {tc.label || `#${tc.position + 1}`}
              </span>
              <span style={{ ...mono, fontSize: '9px', opacity: 0.4 }}>
                {tc.label ? `#${tc.position + 1}` : ''}
              </span>
              {tc.is_sample && (
                <span
                  style={{
                    fontSize: '8px',
                    fontWeight: 700,
                    textTransform: 'uppercase',
                    padding: '0 4px',
                    borderRadius: '3px',
                    background: 'rgba(99,102,241,0.1)',
                    color: '#6366f1',
                  }}
                >
                  S
                </span>
              )}
              <span
                style={{
                  ...mono,
                  fontSize: '9px',
                  opacity: 0.35,
                  marginLeft: 'auto',
                }}
              >
                {tc.score} pts
              </span>
              {otherOwner !== null && (
                <span
                  style={{ fontSize: '9px', fontStyle: 'italic', opacity: 0.6 }}
                >
                  {'\u2192'}{' '}
                  {subtaskNames[otherOwner] ||
                    t('ioi.subtask.defaultName', { index: otherOwner + 1 })}
                </span>
              )}
              {isDup && (
                <span
                  style={{
                    width: '6px',
                    height: '6px',
                    borderRadius: '50%',
                    flexShrink: 0,
                    background: '#ef4444',
                    boxShadow: '0 0 0 2px rgba(239,68,68,0.2)',
                  }}
                />
              )}
            </div>
          );
        })}

        {items.addableTotal > 50 && (
          <div
            style={{
              padding: '5px 10px',
              fontSize: '10px',
              opacity: 0.4,
              textAlign: 'center',
            }}
          >
            {t('ioi.subtask.andMore', { count: items.addableTotal - 50 })}
          </div>
        )}
      </div>,
      portalTarget,
    );
  };

  return (
    <div>
      <input
        ref={inputRef}
        type="text"
        role="combobox"
        aria-expanded={isOpen && hasResults}
        aria-controls={isOpen && hasResults ? listboxId : undefined}
        aria-activedescendant={activeDescendant}
        aria-autocomplete="list"
        aria-haspopup="listbox"
        placeholder={t('ioi.subtask.searchPlaceholder')}
        value={query}
        onChange={(e) => {
          setQuery(e.target.value);
          setShowDropdown(true);
          setActiveIndex(-1);
          measureInput();
        }}
        onFocus={() => {
          cancelBlur();
          measureInput();
          if (q) setShowDropdown(true);
        }}
        onBlur={() => {
          cancelBlur();
          blurTimeoutRef.current = setTimeout(() => {
            setShowDropdown(false);
            setActiveIndex(-1);
          }, 200);
        }}
        onKeyDown={handleKeyDown}
        style={{
          ...fieldInput,
          ...mono,
          width: '100%',
          fontSize: '12px',
          background: th.card,
          boxSizing: 'border-box' as const,
        }}
      />
      {renderDropdown()}
    </div>
  );
}

export function SubtaskEditor({
  value,
  schema,
  onChange,
  scope,
}: SubtaskEditorProps) {
  const { t } = useTranslation();
  const subtasks: SubtaskValue[] = Array.isArray(value)
    ? (value as Record<string, unknown>[]).map((raw) => ({
        name: (raw.name as string) ?? '',
        scoring_method: (raw.scoring_method as string) ?? 'group_min',
        max_score: (raw.max_score as number) ?? 100,
        test_cases: Array.isArray(raw.test_cases)
          ? (raw.test_cases as string[])
          : [],
      }))
    : [];

  // Extract problemId from scope
  const problemId =
    scope && 'problemId' in scope ? (scope.problemId as number) : undefined;
  const {
    testCases,
    loading: tcLoading,
    error: tcError,
    refetch,
  } = useTestCases(problemId);

  // Build lookup maps
  const tcByLabel = useMemo(() => {
    const map = new Map<string, TestCaseListItem>();
    for (const tc of testCases) map.set(tcLabel(tc), tc);
    return map;
  }, [testCases]);

  const tcByPosition = useMemo(() => {
    const map = new Map<number, TestCaseListItem>();
    for (const tc of testCases) map.set(tc.position + 1, tc); // 1-indexed for user display
    return map;
  }, [testCases]);

  // Assignment map: label → all subtask indices that contain it
  const assignmentMap = useMemo(() => {
    const map = new Map<string, number[]>();
    subtasks.forEach((s, i) => {
      for (const label of s.test_cases) {
        const arr = map.get(label);
        if (arr) arr.push(i);
        else map.set(label, [i]);
      }
    });
    return map;
  }, [subtasks]);

  // Validation
  const unassignedLabels = useMemo(
    () =>
      testCases
        .filter((tc) => !assignmentMap.has(tcLabel(tc)))
        .map((tc) => tcLabel(tc)),
    [testCases, assignmentMap],
  );

  const duplicateLabels = useMemo(() => {
    const seen = new Map<string, number[]>();
    subtasks.forEach((s, i) => {
      for (const label of s.test_cases) {
        if (!seen.has(label)) seen.set(label, []);
        seen.get(label)!.push(i);
      }
    });
    return [...seen.entries()].filter(([, indices]) => indices.length > 1);
  }, [subtasks]);

  // O(1) lookup for duplicate labels
  const duplicateSet = useMemo(() => {
    const set = new Set<string>();
    for (const [label] of duplicateLabels) set.add(label);
    return set;
  }, [duplicateLabels]);

  // Subtask names for "-> SubtaskName" labels in dropdown
  const subtaskNames = useMemo(() => subtasks.map((s) => s.name), [subtasks]);

  const duplicateNameIndices = useMemo(() => {
    const nameCount = new Map<string, number[]>();
    subtasks.forEach((s, i) => {
      if (!s.name) return;
      const key = s.name.trim().toLowerCase();
      if (!key) return;
      if (!nameCount.has(key)) nameCount.set(key, []);
      nameCount.get(key)!.push(i);
    });
    const dupes = new Set<number>();
    for (const [, indices] of nameCount) {
      if (indices.length > 1) indices.forEach((i) => dupes.add(i));
    }
    return dupes;
  }, [subtasks]);

  // Stable keys for subtask cards (monotonic counter)
  const nextKeyRef = useRef(subtasks.length);
  const [subtaskKeys, setSubtaskKeys] = useState<number[]>(() =>
    subtasks.map((_, i) => i),
  );
  // Keep keys in sync when subtasks grow or shrink externally
  useEffect(() => {
    setSubtaskKeys((prev) => {
      if (prev.length === subtasks.length) return prev;
      if (prev.length < subtasks.length) {
        const newKeys = [...prev];
        while (newKeys.length < subtasks.length)
          newKeys.push(nextKeyRef.current++);
        return newKeys;
      }
      // External shrinkage
      return prev.slice(0, subtasks.length);
    });
  }, [subtasks.length]);

  // Portal target: find the Radix Dialog portal container so our overlays
  // stay inside the dialog's interactive zone (not marked inert by Radix).
  const rootRef = useRef<HTMLDivElement>(null);
  const [portalInfo, setPortalInfo] = useState<{
    target: HTMLElement;
    offset: { dx: number; dy: number };
  }>({
    target: document.body,
    offset: { dx: 0, dy: 0 },
  });
  useLayoutEffect(() => {
    if (!rootRef.current) return;
    const target = findPortalTarget(rootRef.current);
    const offset = getPortalOffset(target);
    setPortalInfo({ target, offset });
  }, []);

  // State
  const [openPools, setOpenPools] = useState<Record<number, boolean>>({});
  const [warningsExpanded, setWarningsExpanded] = useState(true);
  const [lastClicked, setLastClicked] = useState<string | null>(null);
  const [statusMessage, setStatusMessage] =
    useState<EditorStatusMessage | null>(null);
  const [pendingImport, setPendingImport] = useState<SubtaskValue[] | null>(
    null,
  );
  const dragDataRef = useRef<{
    tcLabels: string[];
    fromSubtask: number | null;
  } | null>(null);
  const [dropTarget, setDropTarget] = useState<number | null>(null);
  const dragCountersRef = useRef<Record<number, number>>({});

  useEffect(() => {
    if (!statusMessage) return;
    const id = window.setTimeout(() => {
      setStatusMessage((current) =>
        current?.text === statusMessage.text ? null : current,
      );
    }, 2400);
    return () => window.clearTimeout(id);
  }, [statusMessage]);

  // Mutations
  const updateSubtask = useCallback(
    (index: number, patch: Partial<SubtaskValue>) => {
      const updated = subtasks.map((s, i) =>
        i === index ? { ...s, ...patch } : s,
      );
      onChange(updated);
    },
    [subtasks, onChange],
  );

  const resetInteractiveState = useCallback(() => {
    setOpenPools({});
    setDropTarget(null);
    setLastClicked(null);
    dragDataRef.current = null;
    dragCountersRef.current = {};
  }, []);

  const removeSubtask = useCallback(
    (index: number) => {
      const name =
        subtasks[index]?.name ||
        t('ioi.subtask.defaultName', { index: index + 1 });
      const count = subtasks[index]?.test_cases.length ?? 0;
      const msg =
        count > 0
          ? t('ioi.subtask.removeConfirm', { name, count })
          : t('ioi.subtask.removeConfirmEmpty', { name });
      if (!window.confirm(msg)) return;
      onChange(subtasks.filter((_, i) => i !== index));
      setSubtaskKeys((prev) => prev.filter((_, i) => i !== index));
      // Shift index-keyed state maps so entries above the removed index move down by 1
      setOpenPools((prev) => {
        const shifted: Record<number, boolean> = {};
        for (const [k, v] of Object.entries(prev)) {
          const ki = Number(k);
          if (ki < index) shifted[ki] = v;
          else if (ki > index) shifted[ki - 1] = v;
        }
        return shifted;
      });
    },
    [subtasks, onChange, t],
  );

  const addSubtask = useCallback(() => {
    onChange([...subtasks, defaultSubtask(subtasks.length, t)]);
    setSubtaskKeys((prev) => [...prev, nextKeyRef.current++]);
  }, [subtasks, onChange, t]);

  const applyImportedSubtasks = useCallback(
    (imported: SubtaskValue[], mode: 'replace' | 'merge') => {
      const startIndex = mode === 'merge' ? subtasks.length : 0;
      const materialized = imported.map((subtask, index) => ({
        ...subtask,
        name: subtask.name.trim() || defaultSubtask(startIndex + index, t).name,
        test_cases: [...subtask.test_cases],
      }));
      const nextSubtasks =
        mode === 'merge' ? [...subtasks, ...materialized] : materialized;

      onChange(nextSubtasks);
      if (mode === 'merge') {
        setSubtaskKeys((prev) => [
          ...prev,
          ...materialized.map(() => nextKeyRef.current++),
        ]);
      } else {
        setSubtaskKeys(materialized.map(() => nextKeyRef.current++));
      }
      setPendingImport(null);
      setStatusMessage(null);
      resetInteractiveState();
    },
    [onChange, resetInteractiveState, subtasks, t],
  );

  const handleCopyJson = useCallback(async () => {
    if (!navigator.clipboard?.writeText) {
      setStatusMessage({
        tone: 'error',
        text: t('ioi.subtask.pasteClipboardError'),
      });
      return;
    }
    try {
      await navigator.clipboard.writeText(JSON.stringify(subtasks, null, 2));
      setPendingImport(null);
      setStatusMessage({
        tone: 'success',
        text: t('ioi.subtask.copySuccess'),
      });
    } catch {
      setStatusMessage({
        tone: 'error',
        text: t('ioi.subtask.pasteClipboardError'),
      });
    }
  }, [subtasks, t]);

  const handlePasteJson = useCallback(async () => {
    if (!navigator.clipboard?.readText) {
      setStatusMessage({
        tone: 'error',
        text: t('ioi.subtask.pasteClipboardError'),
      });
      return;
    }

    try {
      const clipboardText = await navigator.clipboard.readText();
      let parsed: unknown;
      try {
        parsed = JSON.parse(clipboardText);
      } catch {
        setPendingImport(null);
        setStatusMessage({
          tone: 'error',
          text: t('ioi.subtask.pasteInvalidJson'),
        });
        return;
      }

      const normalized = normalizeImportedSubtasks(parsed);
      if (!normalized) {
        setPendingImport(null);
        setStatusMessage({
          tone: 'error',
          text: t('ioi.subtask.pasteInvalidShape'),
        });
        return;
      }

      if (subtasks.length === 0) {
        applyImportedSubtasks(normalized, 'replace');
        return;
      }

      setPendingImport(normalized);
      setStatusMessage(null);
      resetInteractiveState();
    } catch {
      setPendingImport(null);
      setStatusMessage({
        tone: 'error',
        text: t('ioi.subtask.pasteClipboardError'),
      });
    }
  }, [applyImportedSubtasks, resetInteractiveState, subtasks.length, t]);

  // Pool click handler with shift/ctrl support
  const handlePoolClick = useCallback(
    (tc: TestCaseListItem, subtaskIdx: number, e: React.MouseEvent) => {
      const label = tcLabel(tc);
      const owners = assignmentMap.get(label) ?? [];
      // Block click if assigned exclusively to other subtasks (not this one)
      if (owners.length > 0 && !owners.includes(subtaskIdx)) return;

      const isAssigned = subtasks[subtaskIdx].test_cases.includes(label);

      if (e.shiftKey && lastClicked !== null) {
        // Range select from last clicked to this one
        const labels = testCases.map((t) => tcLabel(t));
        const lastIdx = labels.indexOf(lastClicked);
        const thisIdx = labels.indexOf(label);
        if (lastIdx !== -1 && thisIdx !== -1) {
          const lo = Math.min(lastIdx, thisIdx);
          const hi = Math.max(lastIdx, thisIdx);
          const rangeLabels = labels.slice(lo, hi + 1).filter((l) => {
            const o = assignmentMap.get(l) ?? [];
            return o.length === 0 || o.includes(subtaskIdx);
          });

          if (isAssigned) {
            // Deselect range
            updateSubtask(subtaskIdx, {
              test_cases: subtasks[subtaskIdx].test_cases.filter(
                (l) => !rangeLabels.includes(l),
              ),
            });
          } else {
            // Select range
            const existing = new Set(subtasks[subtaskIdx].test_cases);
            const merged = [
              ...subtasks[subtaskIdx].test_cases,
              ...rangeLabels.filter((l) => !existing.has(l)),
            ];
            updateSubtask(subtaskIdx, { test_cases: merged });
          }
        }
      } else {
        // Toggle single
        if (isAssigned) {
          updateSubtask(subtaskIdx, {
            test_cases: subtasks[subtaskIdx].test_cases.filter(
              (l) => l !== label,
            ),
          });
        } else {
          updateSubtask(subtaskIdx, {
            test_cases: [...subtasks[subtaskIdx].test_cases, label],
          });
        }
      }
      setLastClicked(label);
    },
    [assignmentMap, subtasks, testCases, lastClicked, updateSubtask],
  );

  // Drag & drop
  const handleDragStart = useCallback(
    (e: React.DragEvent, tcLabels: string[], fromSubtask: number | null) => {
      e.dataTransfer.setData('text/plain', '');
      e.dataTransfer.effectAllowed = 'move';
      dragDataRef.current = { tcLabels, fromSubtask };
    },
    [],
  );

  // Clean up drag state when any drag ends (drop or cancel).
  // dragend fires on the source element and bubbles to root.
  const handleDragEnd = useCallback(() => {
    dragDataRef.current = null;
    setDropTarget(null);
    dragCountersRef.current = {};
  }, []);

  const handleDrop = useCallback(
    (toSubtask: number) => {
      const data = dragDataRef.current;
      if (!data) return;
      const { tcLabels, fromSubtask } = data;

      // No-op if dropping on the same subtask
      if (fromSubtask === toSubtask) {
        dragDataRef.current = null;
        setDropTarget(null);
        return;
      }

      const updated = subtasks.map((s, i) => {
        if (i === fromSubtask) {
          return {
            ...s,
            test_cases: s.test_cases.filter((l) => !tcLabels.includes(l)),
          };
        }
        if (i === toSubtask) {
          const existing = new Set(s.test_cases);
          return {
            ...s,
            test_cases: [
              ...s.test_cases,
              ...tcLabels.filter((l) => !existing.has(l)),
            ],
          };
        }
        return s;
      });
      onChange(updated);
      dragDataRef.current = null;
      setDropTarget(null);
    },
    [subtasks, onChange],
  );

  const totalMax = subtasks.reduce((sum, s) => sum + (s.max_score || 0), 0);

  return (
    <div
      ref={rootRef}
      onDragEnd={handleDragEnd}
      style={{
        display: 'flex',
        flexDirection: 'column',
        gap: '12px',
        gridColumn: 'span 2',
      }}
    >
      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'space-between',
          gap: '12px',
          flexWrap: 'wrap',
        }}
      >
        <div>
          <span
            style={{
              fontSize: '11px',
              fontWeight: 700,
              textTransform: 'uppercase',
              letterSpacing: '0.06em',
              opacity: 0.45,
            }}
          >
            {schema.title ?? t('ioi.subtask.title')}
          </span>
          {problemId && !tcLoading && testCases.length > 0 && (
            <span
              style={{
                ...mono,
                fontSize: '10px',
                opacity: 0.3,
                marginLeft: '10px',
              }}
            >
              {testCases.length !== 1
                ? t('ioi.subtask.testCasesAvailable', {
                    count: testCases.length,
                  })
                : t('ioi.subtask.testCaseAvailable', {
                    count: testCases.length,
                  })}
            </span>
          )}
        </div>
        <div
          style={{
            display: 'flex',
            alignItems: 'center',
            gap: '8px',
            flexWrap: 'wrap',
            justifyContent: 'flex-end',
          }}
        >
          {subtasks.length > 0 && (
            <>
              <span style={{ ...mono, fontSize: '11px', opacity: 0.4 }}>
                {subtasks.length !== 1
                  ? t('ioi.subtask.subtaskCount', { count: subtasks.length })
                  : t('ioi.subtask.subtaskCountSingular', {
                      count: subtasks.length,
                    })}
              </span>
              <span
                style={{
                  ...mono,
                  fontSize: '11px',
                  fontWeight: 700,
                  padding: '2px 10px',
                  borderRadius: '10px',
                  background:
                    totalMax === 100
                      ? 'rgba(16,185,129,0.1)'
                      : 'rgba(245,158,11,0.1)',
                  color: totalMax === 100 ? '#059669' : '#d97706',
                }}
              >
                {totalMax} pts
              </span>
            </>
          )}
          <button
            type="button"
            onClick={handleCopyJson}
            style={headerActionButton}
          >
            {t('ioi.subtask.copyJson')}
          </button>
          <button
            type="button"
            onClick={handlePasteJson}
            style={headerActionButton}
          >
            {t('ioi.subtask.pasteJson')}
          </button>
        </div>
      </div>

      {statusMessage && (
        <div
          style={{
            padding: '10px 12px',
            borderRadius: '8px',
            fontSize: '12px',
            border:
              statusMessage.tone === 'success'
                ? '1px solid rgba(16,185,129,0.25)'
                : '1px solid rgba(239,68,68,0.25)',
            background:
              statusMessage.tone === 'success'
                ? 'rgba(16,185,129,0.08)'
                : 'rgba(239,68,68,0.08)',
            color: statusMessage.tone === 'success' ? '#047857' : '#dc2626',
          }}
        >
          {statusMessage.text}
        </div>
      )}

      {pendingImport && (
        <div
          style={{
            padding: '12px',
            borderRadius: '10px',
            border: `1px solid ${th.border}`,
            background: `color-mix(in srgb, ${th.muted} 55%, transparent)`,
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'space-between',
            gap: '12px',
            flexWrap: 'wrap',
          }}
        >
          <div
            style={{
              fontSize: '12px',
              fontWeight: 600,
              color: th.foreground,
            }}
          >
            {t('ioi.subtask.importReady', { count: pendingImport.length })}
          </div>
          <div style={{ display: 'flex', gap: '8px', flexWrap: 'wrap' }}>
            <button
              type="button"
              onClick={() => applyImportedSubtasks(pendingImport, 'replace')}
              style={{
                ...headerActionButton,
                borderColor: th.primary,
                color: th.primary,
                opacity: 1,
              }}
            >
              {t('ioi.subtask.importReplace')}
            </button>
            <button
              type="button"
              onClick={() => applyImportedSubtasks(pendingImport, 'merge')}
              style={headerActionButton}
            >
              {t('ioi.subtask.importMerge')}
            </button>
            <button
              type="button"
              onClick={() => setPendingImport(null)}
              style={headerActionButton}
            >
              {t('ioi.subtask.importCancel')}
            </button>
          </div>
        </div>
      )}

      {schema.description && (
        <p
          style={{
            fontSize: '12px',
            opacity: 0.45,
            margin: 0,
            lineHeight: 1.5,
          }}
        >
          {schema.description}
        </p>
      )}

      {problemId && tcLoading && (
        <div
          style={{
            padding: '16px',
            textAlign: 'center',
            fontSize: '12px',
            opacity: 0.5,
            borderRadius: '8px',
            border: `1px solid ${th.border}`,
          }}
        >
          {t('ioi.subtask.loading')}
        </div>
      )}

      {problemId && tcError && (
        <div
          style={{
            padding: '12px 16px',
            borderRadius: '8px',
            background: 'rgba(239,68,68,0.06)',
            border: '1px solid rgba(239,68,68,0.2)',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'space-between',
            fontSize: '12px',
            color: '#dc2626',
          }}
        >
          <span>{tcError}</span>
          <button
            type="button"
            onClick={refetch}
            style={{
              background: 'none',
              border: '1px solid rgba(239,68,68,0.3)',
              borderRadius: '5px',
              padding: '3px 10px',
              fontSize: '11px',
              cursor: 'pointer',
              color: 'inherit',
            }}
          >
            {t('ioi.subtask.retry')}
          </button>
        </div>
      )}

      {!tcLoading && problemId && testCases.length === 0 && (
        <div
          style={{
            textAlign: 'center',
            padding: 16,
            fontSize: 12,
            color: 'var(--muted-foreground, #888)',
            borderRadius: '8px',
            border: `1px dashed ${th.border}`,
          }}
        >
          {t('ioi.subtask.noTestCasesOnProblem')}
        </div>
      )}

      {subtasks.length > 0 &&
        testCases.length > 0 &&
        (unassignedLabels.length > 0 || duplicateLabels.length > 0) && (
          <div
            style={{
              borderRadius: '8px',
              border: `1px solid ${duplicateLabels.length > 0 ? 'rgba(239,68,68,0.4)' : 'rgba(245,158,11,0.3)'}`,
              boxShadow:
                duplicateLabels.length > 0
                  ? '0 0 0 1px rgba(239,68,68,0.15)'
                  : undefined,
              overflow: 'hidden',
            }}
          >
            <button
              type="button"
              onClick={() => setWarningsExpanded((p) => !p)}
              style={{
                width: '100%',
                padding: '7px 12px',
                border: 'none',
                cursor: 'pointer',
                background:
                  duplicateLabels.length > 0
                    ? 'rgba(239,68,68,0.06)'
                    : 'rgba(245,158,11,0.06)',
                color: duplicateLabels.length > 0 ? '#dc2626' : '#b45309',
                display: 'flex',
                alignItems: 'center',
                gap: '6px',
                fontSize: '11px',
                fontWeight: 600,
                textAlign: 'left',
              }}
            >
              <span style={{ fontSize: '13px' }}>
                {warningsExpanded ? '\u25BE' : '\u25B8'}
              </span>
              <span>
                {(() => {
                  const wc =
                    (unassignedLabels.length > 0 ? 1 : 0) +
                    (duplicateLabels.length > 0 ? 1 : 0);
                  return wc !== 1
                    ? t('ioi.subtask.warningCount', { count: wc })
                    : t('ioi.subtask.warningCountSingular', { count: wc });
                })()}
              </span>
              {unassignedLabels.length > 0 && (
                <span style={{ fontWeight: 400, opacity: 0.7 }}>
                  {t('ioi.subtask.unassignedCount', {
                    count: unassignedLabels.length,
                  })}
                </span>
              )}
              {duplicateLabels.length > 0 && (
                <span style={{ fontWeight: 400, opacity: 0.7 }}>
                  {duplicateLabels.length !== 1
                    ? t('ioi.subtask.duplicateCount', {
                        count: duplicateLabels.length,
                      })
                    : t('ioi.subtask.duplicateCountSingular', {
                        count: duplicateLabels.length,
                      })}
                </span>
              )}
            </button>
            {warningsExpanded && (
              <div
                style={{
                  display: 'flex',
                  flexDirection: 'column',
                  gap: '1px',
                  maxHeight: '200px',
                  overflowY: 'auto',
                }}
              >
                {unassignedLabels.length > 0 && (
                  <div
                    style={{
                      padding: '8px 12px',
                      fontSize: '11px',
                      background: 'rgba(245,158,11,0.04)',
                      color: '#b45309',
                      display: 'flex',
                      alignItems: 'center',
                      gap: '6px',
                    }}
                  >
                    <span>
                      {unassignedLabels.length !== 1
                        ? t('ioi.subtask.unassignedMessage', {
                            count: unassignedLabels.length,
                          })
                        : t('ioi.subtask.unassignedMessageSingular', {
                            count: unassignedLabels.length,
                          })}
                    </span>
                  </div>
                )}
                {duplicateLabels.length > 0 && (
                  <div
                    style={{
                      padding: '8px 12px',
                      fontSize: '11px',
                      background: 'rgba(239,68,68,0.04)',
                      color: '#dc2626',
                    }}
                  >
                    <div style={{ fontWeight: 600, marginBottom: '4px' }}>
                      {duplicateLabels.length !== 1
                        ? t('ioi.subtask.duplicateMessage', {
                            count: duplicateLabels.length,
                          })
                        : t('ioi.subtask.duplicateMessageSingular', {
                            count: duplicateLabels.length,
                          })}
                    </div>
                    <div style={{ opacity: 0.8 }}>
                      {duplicateLabels.slice(0, 12).map(([label, indices]) => {
                        const tc = tcByLabel.get(label);
                        const display = tc
                          ? tc.label
                            ? tc.label
                            : `#${tc.position + 1}`
                          : label;
                        const names = indices
                          .map(
                            (i) =>
                              subtasks[i]?.name ||
                              t('ioi.subtask.defaultName', { index: i + 1 }),
                          )
                          .join(', ');
                        return (
                          <div
                            key={label}
                            style={{
                              ...mono,
                              fontSize: '10px',
                              lineHeight: 1.6,
                            }}
                          >
                            {display} {'\u2192'} {names}
                          </div>
                        );
                      })}
                      {duplicateLabels.length > 12 && (
                        <div style={{ fontSize: '10px', opacity: 0.6 }}>
                          {t('ioi.subtask.andMore', {
                            count: duplicateLabels.length - 12,
                          })}
                        </div>
                      )}
                    </div>
                  </div>
                )}
              </div>
            )}
          </div>
        )}

      {subtasks.length === 0 && (
        <div
          style={{
            padding: '28px',
            borderRadius: '10px',
            border: `1.5px dashed ${th.border}`,
            textAlign: 'center',
            background: `color-mix(in srgb, ${th.muted} 40%, transparent)`,
          }}
        >
          <div style={{ fontSize: '13px', opacity: 0.45, marginBottom: '4px' }}>
            {t('ioi.subtask.noSubtasks')}
          </div>
          <div style={{ fontSize: '11px', opacity: 0.3 }}>
            {t('ioi.subtask.noSubtasksHint')}
          </div>
        </div>
      )}

      {subtasks.map((subtask, idx) => {
        const methodInfo = getMethodInfo(subtask.scoring_method);
        const isPoolOpen = openPools[idx] ?? false;
        return (
          <div
            key={subtaskKeys[idx] ?? idx}
            onDragEnter={(e) => {
              e.preventDefault();
              dragCountersRef.current[idx] =
                (dragCountersRef.current[idx] ?? 0) + 1;
              setDropTarget(idx);
            }}
            onDragOver={(e) => {
              e.preventDefault();
            }}
            onDragLeave={() => {
              dragCountersRef.current[idx] =
                (dragCountersRef.current[idx] ?? 0) - 1;
              if (dragCountersRef.current[idx] <= 0) {
                dragCountersRef.current[idx] = 0;
                setDropTarget((prev) => (prev === idx ? null : prev));
              }
            }}
            onDrop={(e) => {
              e.preventDefault();
              dragCountersRef.current[idx] = 0;
              handleDrop(idx);
            }}
            style={{
              border:
                dropTarget === idx
                  ? `1.5px solid ${th.primary}`
                  : `1px solid ${th.border}`,
              borderRadius: '10px',
              background: dropTarget === idx ? 'rgba(79,70,229,0.02)' : th.card,
              transition: 'border-color 0.15s, background 0.15s',
            }}
          >
            <div
              style={{
                height: '3px',
                borderRadius: '10px 10px 0 0',
                background: `linear-gradient(90deg, ${methodInfo.color}, color-mix(in srgb, ${methodInfo.color} 30%, transparent))`,
              }}
            />

            <div
              style={{
                padding: '14px',
                display: 'flex',
                flexDirection: 'column',
                gap: '10px',
              }}
            >
              <div
                style={{
                  display: 'grid',
                  gridTemplateColumns: '1fr auto auto auto',
                  gap: '10px',
                  alignItems: 'end',
                }}
              >
                <div>
                  <div style={fieldLabel}>{t('ioi.subtask.fieldName')}</div>
                  <input
                    type="text"
                    value={subtask.name}
                    onChange={(e) =>
                      updateSubtask(idx, { name: e.target.value })
                    }
                    style={{
                      ...fieldInput,
                      width: '100%',
                      ...(duplicateNameIndices.has(idx)
                        ? { borderColor: 'rgba(245, 158, 11, 0.5)' }
                        : {}),
                    }}
                    maxLength={50}
                  />
                  {duplicateNameIndices.has(idx) && (
                    <div
                      style={{
                        fontSize: '10px',
                        color: '#b45309',
                        marginTop: '2px',
                      }}
                    >
                      {t('ioi.subtask.duplicateName')}
                    </div>
                  )}
                </div>
                <div>
                  <div style={fieldLabel}>{t('ioi.subtask.fieldMethod')}</div>
                  <div style={{ position: 'relative' }}>
                    <select
                      value={subtask.scoring_method}
                      onChange={(e) =>
                        updateSubtask(idx, { scoring_method: e.target.value })
                      }
                      style={{
                        ...fieldInput,
                        minWidth: '130px',
                        paddingLeft: '34px',
                        appearance: 'none',
                        WebkitAppearance: 'none',
                      }}
                    >
                      {SCORING_METHODS.map((m) => (
                        <option key={m.key} value={m.key}>
                          {t(m.labelKey)}
                        </option>
                      ))}
                    </select>
                    <span
                      style={{
                        position: 'absolute',
                        left: '7px',
                        top: '50%',
                        transform: 'translateY(-50%)',
                        ...mono,
                        fontSize: '9px',
                        fontWeight: 800,
                        padding: '1px 5px',
                        borderRadius: '3px',
                        background: methodInfo.bg,
                        color: methodInfo.color,
                        letterSpacing: '0.02em',
                        pointerEvents: 'none',
                      }}
                    >
                      {t(methodInfo.shortKey)}
                    </span>
                  </div>
                </div>
                <div>
                  <div style={fieldLabel}>{t('ioi.subtask.fieldPoints')}</div>
                  <input
                    type="number"
                    min={0}
                    max={10000}
                    step={1}
                    value={subtask.max_score}
                    onChange={(e) =>
                      updateSubtask(idx, {
                        max_score: parseFloat(e.target.value) || 0,
                      })
                    }
                    style={{ ...fieldInput, ...mono, width: '80px' }}
                  />
                </div>
                <button
                  type="button"
                  onClick={() => removeSubtask(idx)}
                  style={{
                    width: '30px',
                    height: '30px',
                    display: 'flex',
                    alignItems: 'center',
                    justifyContent: 'center',
                    borderRadius: '6px',
                    border: '1px solid transparent',
                    background: 'none',
                    cursor: 'pointer',
                    fontSize: '14px',
                    color: 'inherit',
                    opacity: 0.3,
                    transition: 'all 0.15s',
                  }}
                  title={t('ioi.subtask.removeSubtask')}
                  onMouseEnter={(e) => {
                    e.currentTarget.style.opacity = '1';
                    e.currentTarget.style.color = th.destructive;
                    e.currentTarget.style.borderColor = th.destructive;
                    e.currentTarget.style.background = 'rgba(239,68,68,0.06)';
                  }}
                  onMouseLeave={(e) => {
                    e.currentTarget.style.opacity = '0.3';
                    e.currentTarget.style.color = 'inherit';
                    e.currentTarget.style.borderColor = 'transparent';
                    e.currentTarget.style.background = 'none';
                  }}
                >
                  ✕
                </button>
              </div>

              <div
                style={{
                  fontSize: '11px',
                  opacity: 0.35,
                  fontStyle: 'italic',
                  paddingLeft: '2px',
                }}
              >
                {t(methodInfo.hintKey)}
              </div>

              <div
                style={{
                  padding: '12px',
                  borderRadius: '8px',
                  background: th.muted,
                }}
              >
                <div
                  style={{
                    display: 'flex',
                    alignItems: 'center',
                    justifyContent: 'space-between',
                    marginBottom: '8px',
                  }}
                >
                  <span
                    style={{
                      ...fieldLabel,
                      marginBottom: 0,
                      display: 'flex',
                      alignItems: 'center',
                      gap: '8px',
                    }}
                  >
                    <span>{t('ioi.subtask.testCases')}</span>
                    <span style={{ fontWeight: 400, ...mono }}>
                      {t('ioi.subtask.assigned', {
                        count: subtask.test_cases.length,
                      })}
                    </span>
                  </span>
                  {problemId && testCases.length > 0 && (
                    <button
                      type="button"
                      onClick={() =>
                        setOpenPools((p) => ({ ...p, [idx]: !p[idx] }))
                      }
                      style={{
                        background: isPoolOpen
                          ? 'rgba(79,70,229,0.08)'
                          : 'none',
                        border:
                          '1px solid ' +
                          (isPoolOpen ? 'rgba(79,70,229,0.3)' : th.border),
                        borderRadius: '5px',
                        padding: '3px 10px',
                        cursor: 'pointer',
                        fontSize: '10px',
                        fontWeight: 600,
                        color: isPoolOpen ? '#4f46e5' : 'inherit',
                        opacity: isPoolOpen ? 1 : 0.5,
                        transition: 'all 0.15s',
                        textTransform: 'uppercase',
                        letterSpacing: '0.04em',
                      }}
                    >
                      {isPoolOpen
                        ? t('ioi.subtask.hidePool')
                        : t('ioi.subtask.showPool')}
                    </button>
                  )}
                </div>

                {testCases.length > 0 && (
                  <div style={{ marginBottom: '8px' }}>
                    <UnifiedSearch
                      testCases={testCases}
                      subtask={subtask}
                      subtaskIdx={idx}
                      updateSubtask={updateSubtask}
                      tcByPosition={tcByPosition}
                      assignmentMap={assignmentMap}
                      duplicateSet={duplicateSet}
                      subtaskNames={subtaskNames}
                      portalTarget={portalInfo.target}
                      portalOffset={portalInfo.offset}
                    />
                  </div>
                )}

                {/* Assigned chips */}
                <div
                  style={{
                    display: 'flex',
                    flexWrap: 'wrap',
                    gap: '4px',
                    minHeight: '28px',
                    maxHeight: '160px',
                    overflowY: 'auto',
                    alignItems: 'center',
                  }}
                  title="Hover over a chip to preview test case data"
                >
                  {subtask.test_cases.map((tcId) => {
                    const tc = tcByLabel.get(tcId);
                    if (!tc) {
                      // Orphaned label — show as raw string
                      return (
                        <span
                          key={tcId}
                          style={{
                            display: 'inline-flex',
                            alignItems: 'center',
                            gap: '3px',
                            padding: '3px 8px',
                            borderRadius: '6px',
                            fontSize: '11px',
                            ...mono,
                            background: 'rgba(239,68,68,0.06)',
                            border: '1px solid rgba(239,68,68,0.2)',
                            color: '#dc2626',
                          }}
                        >
                          {tcId}
                          <button
                            type="button"
                            onClick={() =>
                              updateSubtask(idx, {
                                test_cases: subtask.test_cases.filter(
                                  (l) => l !== tcId,
                                ),
                              })
                            }
                            style={{
                              background: 'none',
                              border: 'none',
                              cursor: 'pointer',
                              fontSize: '10px',
                              opacity: 0.5,
                              padding: '0 1px',
                              color: 'inherit',
                            }}
                          >
                            ✕
                          </button>
                        </span>
                      );
                    }
                    return (
                      <TcChip
                        key={tcId}
                        tc={tc}
                        onRemove={() =>
                          updateSubtask(idx, {
                            test_cases: subtask.test_cases.filter(
                              (l) => l !== tcId,
                            ),
                          })
                        }
                        onDragStart={(e) => handleDragStart(e, [tcId], idx)}
                        isDuplicate={duplicateSet.has(tcId)}
                        portalTarget={portalInfo.target}
                        portalOffset={portalInfo.offset}
                      />
                    );
                  })}
                  {subtask.test_cases.length === 0 && (
                    <span
                      style={{
                        fontSize: '11px',
                        opacity: 0.3,
                        fontStyle: 'italic',
                      }}
                    >
                      {t('ioi.subtask.noTestCases')}
                    </span>
                  )}
                </div>

                {/* Pool grid */}
                {isPoolOpen && (
                  <div style={{ marginTop: '10px' }}>
                    <div
                      style={{
                        display: 'flex',
                        alignItems: 'center',
                        justifyContent: 'space-between',
                        marginBottom: '8px',
                      }}
                    >
                      <span
                        style={{
                          fontSize: '10px',
                          opacity: 0.4,
                          fontWeight: 600,
                        }}
                      >
                        {t('ioi.subtask.allTestCases')}
                      </span>
                      <div style={{ display: 'flex', gap: '6px' }}>
                        <button
                          type="button"
                          onClick={() => {
                            const unassignedForThis = testCases
                              .filter((tc) => !assignmentMap.has(tcLabel(tc)))
                              .map((tc) => tcLabel(tc));
                            updateSubtask(idx, {
                              test_cases: [
                                ...new Set([
                                  ...subtask.test_cases,
                                  ...unassignedForThis,
                                ]),
                              ],
                            });
                          }}
                          style={{
                            background: 'none',
                            border: `1px solid ${th.border}`,
                            borderRadius: '4px',
                            padding: '2px 8px',
                            cursor: 'pointer',
                            fontSize: '9px',
                            fontWeight: 600,
                            color: 'inherit',
                            opacity: 0.5,
                            textTransform: 'uppercase',
                            letterSpacing: '0.03em',
                          }}
                        >
                          {t('ioi.subtask.addAllUnassigned')}
                        </button>
                        <button
                          type="button"
                          onClick={() => {
                            if (subtask.test_cases.length === 0) return;
                            const n = subtask.test_cases.length;
                            if (
                              window.confirm(
                                t('ioi.subtask.clearConfirm', {
                                  count: n,
                                  name: subtask.name,
                                }),
                              )
                            ) {
                              updateSubtask(idx, { test_cases: [] });
                            }
                          }}
                          style={{
                            background: 'none',
                            border: `1px solid ${th.border}`,
                            borderRadius: '4px',
                            padding: '2px 8px',
                            cursor: 'pointer',
                            fontSize: '9px',
                            fontWeight: 600,
                            color: 'inherit',
                            opacity: 0.5,
                            textTransform: 'uppercase',
                            letterSpacing: '0.03em',
                          }}
                        >
                          {t('ioi.subtask.clear')}
                        </button>
                      </div>
                    </div>
                    <div
                      style={{
                        display: 'grid',
                        gridTemplateColumns:
                          'repeat(auto-fill, minmax(90px, 1fr))',
                        gap: '6px',
                        maxHeight: '300px',
                        overflowY: 'auto',
                      }}
                    >
                      {testCases.map((tc) => {
                        const label = tcLabel(tc);
                        const owners = assignmentMap.get(label) ?? [];
                        const state: 'unassigned' | 'this' | 'other' =
                          owners.length === 0
                            ? 'unassigned'
                            : owners.includes(idx)
                              ? 'this'
                              : 'other';
                        const ownerLabel =
                          state === 'other'
                            ? subtasks[owners[0]]?.name
                            : undefined;

                        return (
                          <PoolCard
                            key={tc.id}
                            tc={tc}
                            state={state}
                            ownerLabel={ownerLabel}
                            onClick={(e) => handlePoolClick(tc, idx, e)}
                            onDragStart={(e) =>
                              handleDragStart(
                                e,
                                [label],
                                state === 'this' ? idx : (owners[0] ?? null),
                              )
                            }
                            isDuplicate={duplicateSet.has(label)}
                            portalTarget={portalInfo.target}
                            portalOffset={portalInfo.offset}
                          />
                        );
                      })}
                    </div>
                  </div>
                )}
              </div>
            </div>
          </div>
        );
      })}

      <button
        type="button"
        onClick={addSubtask}
        style={{
          padding: '14px',
          borderRadius: '10px',
          border: `1.5px dashed ${th.border}`,
          background: 'none',
          cursor: 'pointer',
          fontSize: '12px',
          fontWeight: 600,
          color: 'inherit',
          opacity: 0.4,
          transition: 'all 0.2s',
          letterSpacing: '0.02em',
        }}
        onMouseEnter={(e) => {
          e.currentTarget.style.opacity = '0.8';
          e.currentTarget.style.borderColor = th.primary;
          e.currentTarget.style.color = th.primary;
        }}
        onMouseLeave={(e) => {
          e.currentTarget.style.opacity = '0.4';
          e.currentTarget.style.borderColor = th.border;
          e.currentTarget.style.color = 'inherit';
        }}
      >
        {t('ioi.subtask.addSubtask')}
      </button>
    </div>
  );
}
