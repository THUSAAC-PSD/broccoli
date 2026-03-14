import { useTranslation } from '@broccoli/web-sdk/i18n';
import { useSlotPermissions } from '@broccoli/web-sdk/slot';
import type { Submission, TestCaseResult } from '@broccoli/web-sdk/submission';
import Editor from '@monaco-editor/react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import {
  AlertCircle,
  CheckCircle2,
  ChevronDown,
  Clock,
  FileText,
  Loader2,
  MinusCircle,
  XCircle,
} from 'lucide-react';
import type React from 'react';
import { useEffect, useRef, useState } from 'react';

import { resolveFeedbackVisibility } from './feedback-visibility';
import { useIoiApi } from './hooks/useIoiApi';
import { useIsIoiContest } from './hooks/useIsIoiContest';
import { canViewPrivilegedSubmissionFeedback } from './permissions';
import type {
  ContestInfoResponse,
  SubtaskInfo,
  SubtaskScoreEntry,
  SubtaskScoresResponse,
  TaskConfigResponse,
} from './types';

interface IoiSubmissionResultProps {
  submission?: Submission | null;
  testCases?: TestCaseResult[];
}

type SubmissionResponse = Submission;
type TestCaseResultResponse = TestCaseResult;

type DisplayTestCaseResult = TestCaseResultResponse & {
  isPlaceholder?: boolean;
  label?: string;
};

type DisplaySubtaskResult = {
  subtask: SubtaskInfo;
  score: number;
  testCases: DisplayTestCaseResult[];
};

const MONO: React.CSSProperties = {
  fontVariantNumeric: 'tabular-nums',
  fontFamily:
    'ui-monospace, SFMono-Regular, "SF Mono", Menlo, Consolas, monospace',
};

const LANG_DISPLAY: Record<string, { name: string; color: string }> = {
  cpp: { name: 'C++', color: '#00599c' },
  c: { name: 'C', color: '#555555' },
  python3: { name: 'Python 3', color: '#3572a5' },
  java: { name: 'Java', color: '#b07219' },
  rust: { name: 'Rust', color: '#dea584' },
  go: { name: 'Go', color: '#00add8' },
  javascript: { name: 'JS', color: '#f1e05a' },
  typescript: { name: 'TS', color: '#3178c6' },
};

const METHOD_META: Record<string, { abbrKey: string; color: string }> = {
  group_min: { abbrKey: 'ioi.submission.method.groupMin', color: '#ef4444' },
  sum: { abbrKey: 'ioi.submission.method.sum', color: '#10b981' },
  group_mul: { abbrKey: 'ioi.submission.method.groupMul', color: '#f59e0b' },
};

const VERDICT_META: Record<string, { color: string; bg: string }> = {
  Accepted: { color: '#10b981', bg: 'rgba(16,185,129,0.1)' },
  WrongAnswer: { color: '#ef4444', bg: 'rgba(239,68,68,0.1)' },
  TimeLimitExceeded: { color: '#f59e0b', bg: 'rgba(245,158,11,0.1)' },
  MemoryLimitExceeded: { color: '#f97316', bg: 'rgba(249,115,22,0.1)' },
  RuntimeError: { color: '#a855f7', bg: 'rgba(168,85,247,0.1)' },
  SystemError: { color: '#6b7280', bg: 'rgba(107,114,128,0.1)' },
  Skipped: { color: '#9ca3af', bg: 'rgba(156,163,175,0.08)' },
  Pending: { color: '#94a3b8', bg: 'rgba(148,163,184,0.1)' },
  Running: { color: '#3b82f6', bg: 'rgba(59,130,246,0.1)' },
};

const VERDICT_ICONS = {
  Accepted: CheckCircle2,
  WrongAnswer: XCircle,
  TimeLimitExceeded: Clock,
  MemoryLimitExceeded: Clock,
  RuntimeError: AlertCircle,
  SystemError: AlertCircle,
  Skipped: MinusCircle,
  Pending: Clock,
  Running: Loader2,
} as const;

function VerdictIcon({
  verdict,
  size = 14,
}: {
  verdict: string;
  size?: number;
}) {
  const meta = VERDICT_META[verdict];
  const c = meta?.color ?? '#6b7280';
  const Icon =
    VERDICT_ICONS[verdict as keyof typeof VERDICT_ICONS] ?? AlertCircle;

  return (
    <Icon
      size={size}
      color={c}
      style={{ flexShrink: 0 }}
      className={verdict === 'Running' ? 'animate-spin' : undefined}
    />
  );
}

function scoreColor(score: number, maxScore: number): string {
  if (maxScore <= 0) return '#6b7280';
  const frac = score / maxScore;
  if (frac >= 1) return '#10b981';
  if (frac > 0) return '#f59e0b';
  return '#6b7280';
}

function formatMs(ms: number): string {
  return ms < 1000 ? `${ms}ms` : `${(ms / 1000).toFixed(2)}s`;
}

function formatKb(kb: number): string {
  const mb = kb / 1024;
  return `${mb.toFixed(mb >= 10 ? 0 : 1)} MB`;
}

function clamp01(value: number): number {
  return Math.max(0, Math.min(1, value));
}

function createPlaceholderTestCase(
  label: string,
  testCaseId: number | undefined,
  placeholderId: number,
  verdict: 'Pending' | 'Running',
): DisplayTestCaseResult {
  return {
    id: placeholderId,
    test_case_id: testCaseId ?? placeholderId,
    score: 0,
    verdict,
    isPlaceholder: true,
    label,
  };
}

function buildStaticTestCaseList({
  labels,
  labelMap,
  tcById,
  subtaskIndex,
}: {
  labels: string[];
  labelMap: Record<string, number>;
  tcById: Map<number, TestCaseResultResponse>;
  subtaskIndex: number;
}): DisplayTestCaseResult[] {
  return labels.map((label, labelIndex) => {
    const resolvedId =
      labelMap[label] ??
      (Number.isNaN(Number(label)) ? undefined : Number(label));
    const actual = resolvedId != null ? tcById.get(resolvedId) : undefined;
    if (actual) {
      return actual;
    }

    return createPlaceholderTestCase(
      label,
      resolvedId,
      -((subtaskIndex + 1) * 10000 + labelIndex + 1),
      'Pending',
    );
  });
}

function getNormalizedTestCaseScore(
  testCase: DisplayTestCaseResult,
  maxScore: number | undefined,
): number | null {
  if (testCase.isPlaceholder) {
    return null;
  }
  if (!maxScore || maxScore <= 0) {
    return testCase.verdict === 'Accepted' ? 1 : 0;
  }
  return clamp01(testCase.score / maxScore);
}

function computeProvisionalSubtaskScore(
  subtask: SubtaskInfo,
  testCases: DisplayTestCaseResult[],
  testCaseMaxScores: Record<string, number>,
): number {
  const labels = subtask.test_cases ?? [];
  if (labels.length === 0) {
    return 0;
  }

  const normalized = labels.map((label, index) =>
    getNormalizedTestCaseScore(testCases[index], testCaseMaxScores[label]),
  );
  const judged = normalized.filter((value): value is number => value != null);

  if (judged.length === 0) {
    return 0;
  }

  switch (subtask.scoring_method) {
    case 'group_min':
      return judged.every((value) => value >= 1) ? subtask.max_score : 0;
    case 'group_mul':
      return Number(
        (
          judged.reduce((product, value) => product * value, 1) *
          subtask.max_score
        ).toFixed(2),
      );
    case 'sum':
    default:
      return Number(
        (
          (normalized.reduce((sum, value) => sum + (value ?? 0), 0) /
            labels.length) *
          subtask.max_score
        ).toFixed(2),
      );
  }
}

function buildSubtaskResults({
  taskSubtasks,
  subtaskScores,
  effectiveFeedback,
  labelMap,
  testCaseMaxScores,
  allTestCases,
}: {
  taskSubtasks: SubtaskInfo[];
  subtaskScores: SubtaskScoreEntry[] | null | undefined;
  effectiveFeedback: string;
  labelMap: Record<string, number>;
  testCaseMaxScores: Record<string, number>;
  allTestCases: TestCaseResultResponse[];
}): DisplaySubtaskResult[] {
  const tcById = new Map<number, TestCaseResultResponse>();
  for (const testCase of allTestCases) {
    tcById.set(testCase.test_case_id, testCase);
  }

  const subtaskCount = Math.max(
    taskSubtasks.length,
    subtaskScores?.length ?? 0,
  );
  const results: DisplaySubtaskResult[] = [];

  for (let index = 0; index < subtaskCount; index += 1) {
    const scoreEntry = subtaskScores?.[index];
    const configSubtask = taskSubtasks[index];
    if (!configSubtask && !scoreEntry) {
      continue;
    }

    const subtask: SubtaskInfo = configSubtask ?? {
      name: scoreEntry?.name ?? '',
      scoring_method: scoreEntry?.scoring_method ?? 'sum',
      max_score: scoreEntry?.max_score ?? 0,
    };

    const testCases =
      effectiveFeedback === 'full' && subtask.test_cases?.length
        ? buildStaticTestCaseList({
            labels: subtask.test_cases,
            labelMap,
            tcById,
            subtaskIndex: index,
          })
        : [];

    const score =
      scoreEntry?.score ??
      (testCases.length > 0
        ? computeProvisionalSubtaskScore(subtask, testCases, testCaseMaxScores)
        : 0);

    results.push({
      subtask: {
        name: subtask.name,
        scoring_method: subtask.scoring_method,
        max_score: subtask.max_score,
        test_cases: subtask.test_cases,
      },
      score,
      testCases,
    });
  }

  return results;
}

const MONACO_LANG: Record<string, string> = {
  cpp: 'cpp',
  c: 'c',
  python3: 'python',
  java: 'java',
  rust: 'rust',
  go: 'go',
  javascript: 'javascript',
  typescript: 'typescript',
};

function CodeViewer({
  files,
  language,
}: {
  files: SubmissionResponse['files'];
  language?: string;
}) {
  const [open, setOpen] = useState(false);
  const file = files[0];
  if (!file) return null;

  const lineCount = file.content.split('\n').length;
  const editorHeight = Math.min(Math.max(lineCount * 19, 80), 400);
  const lang = language
    ? (LANG_DISPLAY[language] ?? { name: language, color: '#6b7280' })
    : null;
  const monacoLang = language
    ? (MONACO_LANG[language] ?? language)
    : 'plaintext';

  return (
    <div
      style={{
        borderRadius: 8,
        overflow: 'hidden',
        border: '1px solid var(--border, rgba(0,0,0,0.1))',
      }}
    >
      {/* Header bar */}
      <button
        onClick={() => setOpen(!open)}
        style={{
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'space-between',
          width: '100%',
          padding: '8px 12px',
          border: 'none',
          background: '#1e1e2e',
          cursor: 'pointer',
          gap: 8,
        }}
      >
        <div
          style={{ display: 'flex', alignItems: 'center', gap: 8, minWidth: 0 }}
        >
          {/* File icon */}
          <FileText
            size={14}
            color="#cdd6f4"
            style={{ flexShrink: 0, opacity: 0.5 }}
          />
          <span
            style={{
              ...MONO,
              fontSize: 12,
              color: '#cdd6f4',
              overflow: 'hidden',
              textOverflow: 'ellipsis',
              whiteSpace: 'nowrap',
            }}
          >
            {file.filename}
          </span>
          {lang && (
            <span
              style={{
                padding: '1px 6px',
                borderRadius: 4,
                fontSize: 10,
                fontWeight: 700,
                letterSpacing: '0.03em',
                background: `${lang.color}22`,
                color: lang.color,
                border: `1px solid ${lang.color}44`,
                whiteSpace: 'nowrap',
              }}
            >
              {lang.name}
            </span>
          )}
        </div>
        <ChevronDown
          size={16}
          color="#6c7086"
          style={{
            flexShrink: 0,
            transition: 'transform 0.2s ease',
            transform: open ? 'rotate(180deg)' : 'rotate(0deg)',
          }}
        />
      </button>

      {/* Collapsible body using grid-template-rows trick */}
      <div
        style={{
          display: 'grid',
          gridTemplateRows: open ? '1fr' : '0fr',
          transition: 'grid-template-rows 0.25s ease',
        }}
      >
        <div style={{ overflow: 'hidden' }}>
          <div style={{ height: editorHeight }}>
            {open && (
              <Editor
                height="100%"
                language={monacoLang}
                value={file.content}
                theme="vs-dark"
                options={{
                  readOnly: true,
                  domReadOnly: true,
                  minimap: { enabled: false },
                  fontSize: 13,
                  lineNumbers: 'on',
                  scrollBeyondLastLine: false,
                  renderLineHighlight: 'none',
                  overviewRulerLanes: 0,
                  hideCursorInOverviewRuler: true,
                  overviewRulerBorder: false,
                  scrollbar: { vertical: 'auto', horizontal: 'auto' },
                  contextmenu: false,
                  selectionHighlight: false,
                  occurrencesHighlight: 'off',
                  folding: false,
                  lineDecorationsWidth: 0,
                  padding: { top: 8, bottom: 8 },
                }}
              />
            )}
          </div>
        </div>
      </div>
    </div>
  );
}

function CompileOutput({ output }: { output: string }) {
  const { t } = useTranslation();
  return (
    <div
      style={{
        borderRadius: 8,
        overflow: 'hidden',
        border: '1px solid rgba(239, 68, 68, 0.25)',
        borderLeft: '3px solid #ef4444',
      }}
    >
      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          gap: 6,
          padding: '6px 12px',
          background: '#1e1e2e',
          borderBottom: '1px solid #313244',
        }}
      >
        <AlertCircle size={14} color="#ef4444" style={{ flexShrink: 0 }} />
        <span style={{ fontSize: 12, fontWeight: 600, color: '#f38ba8' }}>
          {t('ioi.submission.compilationError')}
        </span>
      </div>
      <div
        style={{
          background: '#1e1e2e',
          padding: 12,
          maxHeight: 300,
          overflowY: 'auto',
          overflowX: 'auto',
          WebkitOverflowScrolling: 'touch',
        }}
      >
        <pre
          style={{
            ...MONO,
            fontSize: 12,
            lineHeight: '18px',
            color: '#f38ba8',
            margin: 0,
            whiteSpace: 'pre-wrap',
            wordBreak: 'break-word',
          }}
        >
          {output}
        </pre>
      </div>
    </div>
  );
}

function RejectionBanner() {
  const { t } = useTranslation();

  return (
    <div
      style={{
        display: 'flex',
        alignItems: 'center',
        gap: 10,
        padding: '10px 14px',
        borderRadius: 8,
        background: 'rgba(217, 119, 6, 0.06)',
        border: '1px solid rgba(217, 119, 6, 0.2)',
        borderLeft: '3px solid #d97706',
      }}
    >
      <AlertCircle size={18} color="#d97706" style={{ flexShrink: 0 }} />
      <div>
        <div style={{ fontSize: 13, fontWeight: 600, color: '#92400e' }}>
          {t('ioi.submission.rejected.title')}
        </div>
        <div style={{ fontSize: 12, color: '#a16207', marginTop: 1 }}>
          {t('ioi.submission.rejected.reason')}
        </div>
      </div>
    </div>
  );
}

function tcHasDetails(tc: TestCaseResultResponse): boolean {
  return !!(
    tc.checker_output ||
    tc.stdout ||
    tc.stderr ||
    tc.input ||
    tc.expected_output
  );
}

function DetailBlock({ label, content }: { label: string; content: string }) {
  return (
    <div style={{ marginBottom: 8 }}>
      <div
        style={{
          fontSize: 10,
          fontWeight: 600,
          textTransform: 'uppercase',
          letterSpacing: '0.06em',
          color: 'var(--muted-foreground, #94a3b8)',
          marginBottom: 4,
        }}
      >
        {label}
      </div>
      <pre
        style={{
          ...MONO,
          fontSize: 12,
          lineHeight: '18px',
          color: 'var(--foreground, #1e293b)',
          margin: 0,
          padding: '8px 10px',
          borderRadius: 6,
          background: 'var(--muted, rgba(0,0,0,0.03))',
          border: '1px solid var(--border, rgba(0,0,0,0.06))',
          whiteSpace: 'pre-wrap',
          wordBreak: 'break-word',
          maxHeight: 200,
          overflowY: 'auto',
        }}
      >
        {content}
      </pre>
    </div>
  );
}

function TestCaseDetailPanel({
  tc,
  index,
}: {
  tc: TestCaseResultResponse;
  index: number;
}) {
  const vm = VERDICT_META[tc.verdict] ?? {
    color: '#6b7280',
    bg: 'rgba(0,0,0,0.04)',
  };
  const { t } = useTranslation();

  return (
    <div
      style={{
        padding: '10px 12px',
        borderRadius: 6,
        background: vm.bg,
        border: `1px solid ${vm.color}22`,
      }}
    >
      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          gap: 6,
          marginBottom: 8,
          paddingBottom: 8,
          borderBottom: '1px solid var(--border, rgba(0,0,0,0.06))',
        }}
      >
        <VerdictIcon verdict={tc.verdict} size={16} />
        <span
          style={{
            fontSize: 12,
            fontWeight: 600,
            color: 'var(--foreground, #1e293b)',
          }}
        >
          {t('ioi.submission.testCase', { index: index + 1 })}
        </span>
        {tc.score != null && (
          <span
            style={{
              ...MONO,
              fontSize: 11,
              fontWeight: 600,
              color: tc.score > 0 ? '#10b981' : '#6b7280',
              marginLeft: 4,
            }}
          >
            {t('ioi.submission.score', { score: tc.score })}
          </span>
        )}
        <span style={{ flex: 1 }} />
        {tc.time_used != null && (
          <span
            style={{
              ...MONO,
              color: 'var(--muted-foreground, #94a3b8)',
              fontSize: 11,
            }}
          >
            {formatMs(tc.time_used)}
          </span>
        )}
        {tc.memory_used != null && (
          <span
            style={{
              ...MONO,
              color: 'var(--muted-foreground, #94a3b8)',
              fontSize: 11,
            }}
          >
            {formatKb(tc.memory_used)}
          </span>
        )}
      </div>
      {tc.checker_output && (
        <DetailBlock
          label={t('ioi.submission.detail.checkerOutput')}
          content={tc.checker_output}
        />
      )}
      {tc.stdout && (
        <DetailBlock
          label={t('ioi.submission.detail.stdout')}
          content={tc.stdout}
        />
      )}
      {tc.stderr && (
        <DetailBlock
          label={t('ioi.submission.detail.stderr')}
          content={tc.stderr}
        />
      )}
      {tc.input && (
        <DetailBlock
          label={t('ioi.submission.detail.input')}
          content={tc.input}
        />
      )}
      {tc.expected_output && (
        <DetailBlock
          label={t('ioi.submission.detail.expectedOutput')}
          content={tc.expected_output}
        />
      )}
    </div>
  );
}

function TestCaseResultList({
  testCases,
}: {
  testCases: TestCaseResultResponse[];
}) {
  const [selectedTcIndex, setSelectedTcIndex] = useState<number | null>(null);
  const [hoveredTcIndex, setHoveredTcIndex] = useState<number | null>(null);
  const selectedTc =
    selectedTcIndex != null ? testCases[selectedTcIndex] : null;

  return (
    <div
      style={{
        borderRadius: 8,
        overflow: 'hidden',
        border: '1px solid var(--border, rgba(0,0,0,0.08))',
        background: 'var(--card, #fff)',
      }}
    >
      <div
        style={{
          display: 'grid',
          gridTemplateColumns: 'repeat(auto-fill, minmax(180px, 1fr))',
          gap: 3,
          padding: '8px 10px',
        }}
      >
        {testCases.map((tc, i) => {
          const vm = VERDICT_META[tc.verdict] ?? {
            color: '#6b7280',
            bg: 'rgba(0,0,0,0.04)',
          };
          const clickable = tcHasDetails(tc);
          const isSelected = selectedTcIndex === i;
          const tcScore = tc.score ?? 0;
          const tcScoreColor =
            tc.verdict === 'Accepted'
              ? '#10b981'
              : tcScore > 0
                ? '#f59e0b'
                : '#6b7280';

          return (
            <div
              key={tc.id}
              role={clickable ? 'button' : undefined}
              tabIndex={clickable ? 0 : undefined}
              onClick={
                clickable
                  ? () => setSelectedTcIndex(isSelected ? null : i)
                  : undefined
              }
              onKeyDown={
                clickable
                  ? (e) => {
                      if (e.key === 'Enter' || e.key === ' ') {
                        e.preventDefault();
                        setSelectedTcIndex(isSelected ? null : i);
                      }
                    }
                  : undefined
              }
              onMouseEnter={clickable ? () => setHoveredTcIndex(i) : undefined}
              onMouseLeave={
                clickable ? () => setHoveredTcIndex(null) : undefined
              }
              style={{
                display: 'flex',
                alignItems: 'center',
                gap: 6,
                padding: '4px 8px',
                borderRadius: 6,
                fontSize: 12,
                background:
                  isSelected || hoveredTcIndex === i ? `${vm.color}20` : vm.bg,
                transition: 'all 0.15s ease',
                cursor: clickable ? 'pointer' : 'default',
                outline: isSelected ? `1.5px solid ${vm.color}66` : 'none',
                borderBottom: clickable
                  ? `1.5px solid ${isSelected ? vm.color + '66' : vm.color + '30'}`
                  : 'none',
              }}
            >
              <VerdictIcon verdict={tc.verdict} size={14} />
              <span
                style={{
                  color: 'var(--muted-foreground, #64748b)',
                  fontSize: 11,
                }}
              >
                #{i + 1}
              </span>
              {tc.score != null && (
                <span
                  style={{
                    ...MONO,
                    fontSize: 10,
                    fontWeight: 600,
                    color: tcScoreColor,
                  }}
                >
                  {tc.score}
                </span>
              )}
              <span style={{ flex: 1 }} />
              {tc.time_used != null && (
                <span
                  style={{
                    ...MONO,
                    color: 'var(--muted-foreground, #94a3b8)',
                    fontSize: 10,
                  }}
                >
                  {formatMs(tc.time_used)}
                </span>
              )}
              {tc.memory_used != null && (
                <span
                  style={{
                    ...MONO,
                    color: 'var(--muted-foreground, #94a3b8)',
                    fontSize: 10,
                  }}
                >
                  {formatKb(tc.memory_used)}
                </span>
              )}
              {clickable && (
                <ChevronDown
                  size={10}
                  color={vm.color}
                  style={{
                    flexShrink: 0,
                    transition: 'transform 0.2s ease',
                    transform: isSelected ? 'rotate(180deg)' : 'rotate(0deg)',
                    opacity: 0.5,
                  }}
                />
              )}
            </div>
          );
        })}
      </div>

      {selectedTc && selectedTcIndex != null && (
        <div style={{ padding: '0 10px 10px' }}>
          <TestCaseDetailPanel tc={selectedTc} index={selectedTcIndex} />
        </div>
      )}
    </div>
  );
}

function TotalScoreSummary({
  totalScore,
  maxScore,
  tokened,
}: {
  totalScore: number;
  maxScore: number;
  tokened?: boolean;
}) {
  const { t } = useTranslation();

  return (
    <div
      style={{
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        padding: '16px 20px',
        borderRadius: 8,
        background: 'var(--muted, rgba(0,0,0,0.02))',
        border: '1px solid var(--border, rgba(0,0,0,0.08))',
      }}
    >
      <div
        style={{
          display: 'flex',
          flexDirection: 'column',
          alignItems: 'center',
          justifyContent: 'center',
          textAlign: 'center',
          width: '100%',
        }}
      >
        <div
          style={{
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            gap: 6,
            fontSize: 10,
            fontWeight: 600,
            textTransform: 'uppercase',
            letterSpacing: '0.08em',
            color: 'var(--muted-foreground, #94a3b8)',
            marginBottom: 6,
          }}
        >
          {t('ioi.submission.totalScore')}
          {tokened && <TokenedBadge />}
        </div>
        <div
          style={{
            ...MONO,
            display: 'flex',
            alignItems: 'baseline',
            justifyContent: 'center',
            fontSize: 24,
            fontWeight: 700,
            color: scoreColor(totalScore, maxScore),
          }}
        >
          {totalScore.toFixed(totalScore === Math.floor(totalScore) ? 0 : 2)}
          <span
            style={{
              fontWeight: 400,
              fontSize: 16,
              color: 'var(--muted-foreground, #94a3b8)',
            }}
          >
            /{maxScore.toFixed(maxScore === Math.floor(maxScore) ? 0 : 2)}
          </span>
        </div>
      </div>
    </div>
  );
}

function SubtaskCard({
  subtask,
  score,
  testCases,
  feedbackLevel,
  index,
}: {
  subtask: SubtaskInfo;
  score: number;
  testCases: TestCaseResultResponse[];
  feedbackLevel: string;
  index: number;
}) {
  const [listExpanded, setListExpanded] = useState(false);
  const [selectedTcIndex, setSelectedTcIndex] = useState<number | null>(null);
  const [hoveredTcIndex, setHoveredTcIndex] = useState<number | null>(null);
  const maxScore = subtask.max_score;
  const frac = maxScore > 0 ? score / maxScore : 0;
  const color = scoreColor(score, maxScore);
  const { t } = useTranslation();
  const methodRaw = METHOD_META[subtask.scoring_method] ?? {
    abbrKey: '?',
    color: '#6b7280',
  };
  const method = {
    abbr: methodRaw.abbrKey.startsWith('ioi.')
      ? t(methodRaw.abbrKey)
      : methodRaw.abbrKey,
    color: methodRaw.color,
  };

  const INITIAL_VISIBLE = 6;
  const showExpand = testCases.length > INITIAL_VISIBLE;
  const visibleTCs = listExpanded
    ? testCases
    : testCases.slice(0, INITIAL_VISIBLE);
  const selectedTc =
    selectedTcIndex != null ? testCases[selectedTcIndex] : null;

  return (
    <div
      style={{
        borderRadius: 8,
        overflow: 'hidden',
        border: '1px solid var(--border, rgba(0,0,0,0.08))',
        borderLeft: `3px solid ${color}`,
        background: 'var(--card, #fff)',
      }}
    >
      {/* Header */}
      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'space-between',
          padding: '10px 14px',
          gap: 8,
        }}
      >
        <div
          style={{ display: 'flex', alignItems: 'center', gap: 8, minWidth: 0 }}
        >
          <div style={{ minWidth: 0 }}>
            <div
              style={{
                fontSize: 13,
                fontWeight: 600,
                color: 'var(--foreground, #1e293b)',
              }}
            >
              {subtask.name ||
                t('ioi.submission.subtaskFallback', { index: index + 1 })}
            </div>
          </div>
          <span
            style={{
              ...MONO,
              padding: '1px 5px',
              borderRadius: 4,
              fontSize: 9,
              fontWeight: 700,
              letterSpacing: '0.06em',
              background: `${method.color}14`,
              color: method.color,
            }}
          >
            {method.abbr}
          </span>
        </div>
        <span
          style={{
            ...MONO,
            fontSize: 14,
            fontWeight: 700,
            color,
            whiteSpace: 'nowrap',
          }}
        >
          {score.toFixed(score === Math.floor(score) ? 0 : 2)}
          <span
            style={{
              fontWeight: 400,
              fontSize: 12,
              color: 'var(--muted-foreground, #94a3b8)',
            }}
          >
            /{maxScore.toFixed(maxScore === Math.floor(maxScore) ? 0 : 2)}
          </span>
        </span>
      </div>

      {/* Progress bar */}
      <div
        style={{
          height: 3,
          background: 'var(--muted, rgba(0,0,0,0.04))',
        }}
      >
        <div
          style={{
            height: '100%',
            width: `${Math.min(frac * 100, 100)}%`,
            background: `linear-gradient(90deg, ${color}cc, ${color})`,
            borderRadius: '0 2px 2px 0',
            transition: 'width 0.4s ease',
          }}
        />
      </div>

      {/* Test cases (full feedback only) */}
      {feedbackLevel === 'full' && testCases.length > 0 && (
        <div style={{ padding: '8px 10px' }}>
          <div
            style={{
              display: 'grid',
              gridTemplateColumns: 'repeat(auto-fill, minmax(180px, 1fr))',
              gap: 3,
            }}
          >
            {visibleTCs.map((tc, i) => {
              const vm = VERDICT_META[tc.verdict] ?? {
                color: '#6b7280',
                bg: 'rgba(0,0,0,0.04)',
              };
              const clickable = tcHasDetails(tc);
              const isSelected = selectedTcIndex === i;
              const tcScore = tc.score ?? 0;
              const tcScoreColor =
                tc.verdict === 'Accepted'
                  ? '#10b981'
                  : tcScore > 0
                    ? '#f59e0b'
                    : '#6b7280';
              return (
                <div
                  key={tc.id}
                  role={clickable ? 'button' : undefined}
                  tabIndex={clickable ? 0 : undefined}
                  onClick={
                    clickable
                      ? () => setSelectedTcIndex(isSelected ? null : i)
                      : undefined
                  }
                  onKeyDown={
                    clickable
                      ? (e) => {
                          if (e.key === 'Enter' || e.key === ' ') {
                            e.preventDefault();
                            setSelectedTcIndex(isSelected ? null : i);
                          }
                        }
                      : undefined
                  }
                  style={{
                    display: 'flex',
                    alignItems: 'center',
                    gap: 6,
                    padding: '4px 8px',
                    borderRadius: 6,
                    fontSize: 12,
                    background:
                      isSelected || hoveredTcIndex === i
                        ? `${vm.color}20`
                        : vm.bg,
                    transition: 'all 0.15s ease',
                    cursor: clickable ? 'pointer' : 'default',
                    outline: isSelected ? `1.5px solid ${vm.color}66` : 'none',
                    borderBottom: clickable
                      ? `1.5px solid ${isSelected ? vm.color + '66' : vm.color + '30'}`
                      : 'none',
                  }}
                  onMouseEnter={
                    clickable ? () => setHoveredTcIndex(i) : undefined
                  }
                  onMouseLeave={
                    clickable ? () => setHoveredTcIndex(null) : undefined
                  }
                >
                  <VerdictIcon verdict={tc.verdict} size={14} />
                  <span
                    style={{
                      color: 'var(--muted-foreground, #64748b)',
                      fontSize: 11,
                    }}
                  >
                    #{i + 1}
                  </span>
                  {tc.score != null && (
                    <span
                      style={{
                        ...MONO,
                        fontSize: 10,
                        fontWeight: 600,
                        color: tcScoreColor,
                      }}
                    >
                      {tc.score}
                    </span>
                  )}
                  <span style={{ flex: 1 }} />
                  {tc.time_used != null && (
                    <span
                      style={{
                        ...MONO,
                        color: 'var(--muted-foreground, #94a3b8)',
                        fontSize: 10,
                      }}
                    >
                      {formatMs(tc.time_used)}
                    </span>
                  )}
                  {tc.memory_used != null && (
                    <span
                      style={{
                        ...MONO,
                        color: 'var(--muted-foreground, #94a3b8)',
                        fontSize: 10,
                      }}
                    >
                      {formatKb(tc.memory_used)}
                    </span>
                  )}
                  {clickable && (
                    <ChevronDown
                      size={10}
                      color={vm.color}
                      style={{
                        flexShrink: 0,
                        transition: 'transform 0.2s ease',
                        transform: isSelected
                          ? 'rotate(180deg)'
                          : 'rotate(0deg)',
                        opacity: 0.5,
                      }}
                    />
                  )}
                </div>
              );
            })}
          </div>

          {/* Expandable detail panel for selected test case */}
          {selectedTc && selectedTcIndex != null && (
            <div style={{ marginTop: 6 }}>
              <TestCaseDetailPanel tc={selectedTc} index={selectedTcIndex} />
            </div>
          )}

          {showExpand && (
            <button
              onClick={() => {
                if (
                  listExpanded &&
                  selectedTcIndex != null &&
                  selectedTcIndex >= INITIAL_VISIBLE
                ) {
                  setSelectedTcIndex(null);
                }
                setListExpanded(!listExpanded);
              }}
              style={{
                marginTop: 4,
                padding: '4px 10px',
                border: 'none',
                borderRadius: 4,
                background: 'var(--muted, rgba(0,0,0,0.03))',
                color: 'var(--primary, #3b82f6)',
                fontSize: 11,
                fontWeight: 500,
                cursor: 'pointer',
              }}
            >
              {listExpanded
                ? t('ioi.submission.showLess')
                : t('ioi.submission.showAll', { count: testCases.length })}
            </button>
          )}
        </div>
      )}
    </div>
  );
}

function TokenedBadge() {
  const { t } = useTranslation();
  return (
    <span
      style={{
        padding: '2px 8px',
        borderRadius: 10,
        fontSize: 10,
        fontWeight: 600,
        background: 'rgba(59, 130, 246, 0.1)',
        color: '#3b82f6',
        textTransform: 'none',
        letterSpacing: 'normal',
      }}
    >
      {t('ioi.submission.tokened')}
    </span>
  );
}

function LoadingSkeleton() {
  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
      <style>
        {`@keyframes ioi-pulse { 0%, 100% { opacity: 0.4; } 50% { opacity: 0.8; } }`}
      </style>
      {[120, 200, 160].map((w, i) => (
        <div
          key={i}
          style={{
            height: 14,
            width: w,
            borderRadius: 4,
            background: 'var(--muted, #e5e7eb)',
            animation: 'ioi-pulse 1.5s ease-in-out infinite',
            animationDelay: `${i * 150}ms`,
          }}
        />
      ))}
    </div>
  );
}

const STATUS_STYLES: Record<string, { bg: string; color: string }> = {
  Pending: { bg: 'rgba(107, 114, 128, 0.1)', color: '#6b7280' },
  Running: { bg: 'rgba(59, 130, 246, 0.1)', color: '#3b82f6' },
  Judged: { bg: 'rgba(16, 185, 129, 0.1)', color: '#10b981' },
  SystemError: { bg: 'rgba(239, 68, 68, 0.1)', color: '#ef4444' },
  CompilationError: { bg: 'rgba(239, 68, 68, 0.1)', color: '#ef4444' },
  Rejected: { bg: 'rgba(239, 68, 68, 0.1)', color: '#ef4444' },
};

function SubmissionStatusBadge({ status }: { status: string }) {
  const { t } = useTranslation();
  const s = STATUS_STYLES[status] ?? STATUS_STYLES.Pending;
  return (
    <div
      style={{
        display: 'inline-flex',
        alignItems: 'center',
        gap: 6,
        padding: '6px 12px',
        borderRadius: 6,
        background: s.bg,
        color: s.color,
        fontSize: 12,
        fontWeight: 600,
        alignSelf: 'flex-start',
      }}
    >
      {status === 'Running' && (
        <Loader2 size={12} color={s.color} className="animate-spin" />
      )}
      {t(`ioi.submission.status.${status}`, status)}
    </div>
  );
}

const TERMINAL_STATUSES = new Set([
  'Judged',
  'CompilationError',
  'SystemError',
  'Rejected',
]);

export function IoiSubmissionResult({
  submission,
  testCases,
}: IoiSubmissionResultProps) {
  const contestId = submission?.contest_id;
  const problemId = submission?.problem_id;
  const { isIoi, isLoading: guardLoading } = useIsIoiContest(
    contestId ?? undefined,
  );
  const api = useIoiApi();
  const queryClient = useQueryClient();
  const { t } = useTranslation();
  const slotPermissions = useSlotPermissions();
  const hasPrivilegedSubmissionAccess = canViewPrivilegedSubmissionFeedback(
    slotPermissions?.permissions,
  );

  // Invalidate submission-status when a submission reaches terminal status.
  const prevStatusRef = useRef(submission?.status);
  useEffect(() => {
    const prev = prevStatusRef.current;
    const curr = submission?.status;
    prevStatusRef.current = curr;
    if (
      curr &&
      TERMINAL_STATUSES.has(curr) &&
      prev !== curr &&
      contestId &&
      problemId
    ) {
      queryClient.invalidateQueries({
        queryKey: ['ioi-submission-status', contestId, problemId],
      });
    }
  }, [submission?.status, contestId, problemId, queryClient]);

  const taskConfigQuery = useQuery<TaskConfigResponse>({
    queryKey: ['ioi-task-config', contestId, problemId],
    enabled: !!contestId && !!problemId && isIoi,
    queryFn: () => api.getTaskConfig(contestId!, problemId!),
    staleTime: 5 * 60 * 1000,
    retry: 2,
  });
  const taskConfig = taskConfigQuery.data;

  const contestInfoQuery = useQuery<ContestInfoResponse>({
    queryKey: ['ioi-contest-info', contestId],
    enabled: !!contestId && isIoi,
    queryFn: () => api.getContestInfo(contestId!),
    staleTime: 5 * 60 * 1000,
    retry: 2,
  });
  const contestInfo = contestInfoQuery.data;

  const { data: tokenStatus } = useQuery({
    queryKey: ['ioi-token-status', contestId],
    enabled:
      !!contestId &&
      isIoi &&
      !!taskConfig &&
      resolveFeedbackVisibility({
        taskConfig,
        contestInfo,
        isTokened: false,
        canViewPrivilegedSubmissionFeedback: hasPrivilegedSubmissionAccess,
      }).needsTokenStatus,
    queryFn: () => api.getTokenStatus(contestId!),
    staleTime: 60000,
  });

  const isTokened =
    tokenStatus?.tokened_submission_ids?.includes(submission?.id ?? -1) ??
    false;
  const visibility = taskConfig
    ? resolveFeedbackVisibility({
        taskConfig,
        contestInfo,
        isTokened,
        canViewPrivilegedSubmissionFeedback: hasPrivilegedSubmissionAccess,
      })
    : null;
  const feedbackNeedsSubtasks =
    visibility?.effectiveFeedback === 'subtask_scores' ||
    visibility?.effectiveFeedback === 'full';
  const subtaskScoresQuery = useQuery<SubtaskScoresResponse>({
    queryKey: ['ioi-subtask-scores', contestId, submission?.id],
    enabled: !!contestId && !!submission?.id && isIoi && feedbackNeedsSubtasks,
    queryFn: () => api.getSubmissionSubtaskScores(contestId!, submission!.id),
    retry: 2,
  });
  const subtaskScoresData = subtaskScoresQuery.data;

  if (guardLoading || !isIoi) return null;
  if (!submission) return null;

  const isCompileError = submission.status === 'CompilationError';
  const isRejected = submission.status === 'Rejected';

  // Source code viewer (always available if files present)
  const codeViewer =
    submission.files && submission.files.length > 0 ? (
      <CodeViewer files={submission.files} language={submission.language} />
    ) : null;

  // Compilation error (always shown regardless of feedback level)
  if (isCompileError) {
    const compileOutput = submission.result?.compile_output;
    return (
      <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
        {codeViewer}
        {compileOutput && <CompileOutput output={compileOutput} />}
        {!compileOutput && (
          <div
            style={{
              padding: '10px 14px',
              borderRadius: 8,
              borderLeft: '3px solid #ef4444',
              background: 'rgba(239, 68, 68, 0.06)',
              fontSize: 13,
              fontWeight: 600,
              color: '#dc2626',
            }}
          >
            {t('ioi.submission.compilationError')}
          </div>
        )}
      </div>
    );
  }

  // Rejection (verdict is null — submission was not judged)
  if (isRejected) {
    return (
      <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
        {codeViewer}
        <RejectionBanner />
      </div>
    );
  }

  // Loading state for task config (P9)
  if (!taskConfig && taskConfigQuery.isLoading) {
    return (
      <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
        {codeViewer}
        <LoadingSkeleton />
      </div>
    );
  }

  // Error state for task config (P9)
  if (!taskConfig && taskConfigQuery.isError) {
    return (
      <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
        {codeViewer}
        <div
          style={{
            padding: '10px 14px',
            borderRadius: 6,
            background: 'rgba(245, 158, 11, 0.06)',
            border: '1px solid rgba(245, 158, 11, 0.2)',
            fontSize: 12,
            color: '#b45309',
          }}
        >
          {t('ioi.submission.configLoadError')}
        </div>
        {submission?.result?.score != null && (
          <div
            style={{
              textAlign: 'center',
              padding: '12px',
              ...MONO,
              fontSize: 20,
              fontWeight: 700,
              color: 'var(--foreground, #111)',
            }}
          >
            {submission.result.score.toFixed(
              submission.result.score === Math.floor(submission.result.score)
                ? 0
                : 2,
            )}
          </div>
        )}
      </div>
    );
  }

  if (!submission.result || !taskConfig) return codeViewer;

  const allTestCases = testCases ?? submission.result.test_case_results ?? [];
  const { effectiveFeedback } = visibility;
  const taskSubtasks = taskConfig.subtasks ?? [];
  const subtaskScores = subtaskScoresData?.subtasks;
  const labelMap: Record<string, number> = taskConfig.label_map ?? {};
  const testCaseMaxScores: Record<string, number> =
    taskConfig.test_case_max_scores ?? {};
  const subtaskResults = buildSubtaskResults({
    taskSubtasks,
    subtaskScores,
    effectiveFeedback,
    labelMap,
    testCaseMaxScores,
    allTestCases,
  });

  if (effectiveFeedback === 'none') {
    return (
      <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
        {codeViewer}
        <SubmissionStatusBadge status={submission.status} />
        {submission.status === 'SystemError' &&
          submission.result?.system_error && (
            <div
              style={{
                padding: '8px 12px',
                borderRadius: 6,
                fontSize: 12,
                color: '#dc2626',
                background: 'rgba(239, 68, 68, 0.06)',
              }}
            >
              {submission.result.system_error}
            </div>
          )}
        <div
          style={{
            padding: 20,
            textAlign: 'center',
            color: 'var(--muted-foreground, #94a3b8)',
            fontSize: 13,
            fontStyle: 'italic',
          }}
        >
          {t('ioi.submission.noFeedback')}
        </div>
      </div>
    );
  }

  // Compute max possible score from task config subtasks (fallback to 100 if not available)
  const configMaxScore =
    taskSubtasks.length > 0
      ? taskSubtasks.reduce((sum, s) => sum + s.max_score, 0)
      : 100;
  const liveStatusBadge =
    submission.status === 'Pending' ||
    submission.status === 'Compiling' ||
    submission.status === 'Running' ? (
      <SubmissionStatusBadge status={submission.status} />
    ) : null;

  // Feedback: total_only
  if (effectiveFeedback === 'total_only') {
    const totalScore = submission.result.score ?? 0;
    return (
      <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
        {codeViewer}
        {liveStatusBadge}
        <TotalScoreSummary
          totalScore={totalScore}
          maxScore={configMaxScore}
          tokened={visibility.usesTokenMode && isTokened}
        />
      </div>
    );
  }

  // Feedback: subtask_scores or full
  if (subtaskResults.length === 0) {
    const totalScore = submission.result.score ?? 0;

    if (effectiveFeedback === 'full' && allTestCases.length > 0) {
      return (
        <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
          {codeViewer}
          {liveStatusBadge}
          <TotalScoreSummary
            totalScore={totalScore}
            maxScore={configMaxScore}
            tokened={visibility.usesTokenMode && isTokened}
          />
          <TestCaseResultList testCases={allTestCases} />
        </div>
      );
    }

    return (
      <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
        {codeViewer}
        {liveStatusBadge}
        <TotalScoreSummary totalScore={totalScore} maxScore={configMaxScore} />
      </div>
    );
  }

  const totalScore = subtaskResults.reduce(
    (sum: number, r: { score: number }) => sum + r.score,
    0,
  );
  const maxPossible = subtaskResults.reduce(
    (sum: number, result) => sum + result.subtask.max_score,
    0,
  );

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
      {codeViewer}
      {liveStatusBadge}

      {/* Total score summary bar */}
      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'space-between',
          padding: '8px 14px',
          borderRadius: 8,
          background: 'var(--muted, rgba(0,0,0,0.02))',
          border: '1px solid var(--border, rgba(0,0,0,0.08))',
        }}
      >
        <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
          <span
            style={{
              display: 'flex',
              alignItems: 'center',
              gap: 6,
              fontSize: 11,
              fontWeight: 600,
              textTransform: 'uppercase',
              letterSpacing: '0.06em',
              color: 'var(--muted-foreground, #94a3b8)',
            }}
          >
            {t('ioi.submission.total')}
            {visibility.usesTokenMode && isTokened && <TokenedBadge />}
          </span>
        </div>
        <span
          style={{
            ...MONO,
            fontSize: 16,
            fontWeight: 700,
            color: scoreColor(totalScore, maxPossible),
          }}
        >
          {totalScore.toFixed(totalScore === Math.floor(totalScore) ? 0 : 2)}
          <span
            style={{
              fontWeight: 400,
              fontSize: 13,
              color: 'var(--muted-foreground, #94a3b8)',
            }}
          >
            /
            {maxPossible.toFixed(
              maxPossible === Math.floor(maxPossible) ? 0 : 2,
            )}
          </span>
        </span>
      </div>

      {/* Subtask cards */}
      {subtaskResults.map(
        (
          r: {
            subtask: SubtaskInfo;
            score: number;
            testCases: TestCaseResultResponse[];
          },
          i: number,
        ) => (
          <SubtaskCard
            key={i}
            subtask={r.subtask}
            score={r.score}
            testCases={r.testCases}
            feedbackLevel={effectiveFeedback}
            index={i}
          />
        ),
      )}

      {/* Resource usage footer */}
      {(submission.result.time_used != null ||
        submission.result.memory_used != null) && (
        <div
          style={{
            display: 'flex',
            justifyContent: 'flex-end',
            gap: 12,
            padding: '4px 0',
          }}
        >
          {submission.result.time_used != null && (
            <span
              style={{
                ...MONO,
                fontSize: 11,
                color: 'var(--muted-foreground, #94a3b8)',
              }}
            >
              {formatMs(submission.result.time_used)}
            </span>
          )}
          {submission.result.memory_used != null && (
            <span
              style={{
                ...MONO,
                fontSize: 11,
                color: 'var(--muted-foreground, #94a3b8)',
              }}
            >
              {formatKb(submission.result.memory_used)}
            </span>
          )}
        </div>
      )}
    </div>
  );
}
