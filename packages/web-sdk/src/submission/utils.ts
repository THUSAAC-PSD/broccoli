import type {
  SubmissionError,
  SubmissionStatus,
  SubmissionStatusFilterValue,
} from '@/submission/types';

/**
 * Statuses after which a submission or code run will never change again.
 * Pollers should stop once one of these is reached.
 */
export const TERMINAL_STATUSES: ReadonlySet<SubmissionStatus> =
  new Set<SubmissionStatus>(['Judged', 'CompilationError', 'SystemError']);

export function isTerminalStatus(status: SubmissionStatus): boolean {
  return TERMINAL_STATUSES.has(status);
}

function parseSubmissionErrorValue(value: unknown): SubmissionError | null {
  if (!value) return null;
  if (typeof value === 'string') {
    try {
      return parseSubmissionErrorValue(JSON.parse(value));
    } catch {
      return null;
    }
  }
  if (typeof value !== 'object') return null;
  const obj = value as Record<string, unknown>;
  if (typeof obj.code === 'string' && typeof obj.message === 'string') {
    const result: SubmissionError = { code: obj.code, message: obj.message };
    if (obj.details != null && typeof obj.details === 'object') {
      result.details = obj.details as Record<string, unknown>;
    }
    return result;
  }
  return (
    parseSubmissionErrorValue(obj.error) ??
    parseSubmissionErrorValue(obj.data) ??
    parseSubmissionErrorValue(obj.body) ??
    parseSubmissionErrorValue(obj.response)
  );
}

/**
 * Best-effort parse of the server's `{ code, message, details? }` error
 * envelope, unwrapping common nesting (`error`/`data`/`body`/`response`)
 * and JSON-encoded strings along the way.
 */
export function parseSubmissionError(err: unknown): SubmissionError {
  const parsed = parseSubmissionErrorValue(err);
  if (parsed) return parsed;
  return { code: 'UNKNOWN', message: String(err) };
}

export function getStatusLabel(
  status: SubmissionStatusFilterValue,
  t: (key: string) => string,
) {
  switch (status) {
    case 'all':
      return t('submissions.filters.allStatuses');
    case 'Pending':
      return t('result.pending');
    case 'Compiling':
      return t('result.compilingShort');
    case 'Running':
      return t('result.runningShort');
    case 'Judged':
      return t('result.judged');
    case 'CompilationError':
      return t('result.compilationError');
    default:
      return t('result.systemError');
  }
}

export function toSubmissionStatus(
  value: SubmissionStatusFilterValue,
): SubmissionStatus | undefined {
  return value === 'all' ? undefined : value;
}
