import type { SubmissionStatus, Verdict } from '@broccoli/web-sdk/submission';
import type { BadgeProps } from '@broccoli/web-sdk/ui';

type BadgeVariant = NonNullable<BadgeProps['variant']>;

function getStatusLabel(status: SubmissionStatus, t: (key: string) => string) {
  switch (status) {
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
    case 'SystemError':
      return t('result.systemError');
    default:
      return t('result.unknownStatus');
  }
}

function getVerdictLabel(verdict: Verdict, t: (key: string) => string) {
  switch (verdict) {
    case 'Accepted':
      return t('result.accepted');
    case 'WrongAnswer':
      return t('result.wrongAnswer');
    case 'TimeLimitExceeded':
      return t('result.timeLimit');
    case 'MemoryLimitExceeded':
      return t('result.memoryLimit');
    case 'RuntimeError':
      return t('result.runtimeError');
    case 'SystemError':
      return t('result.systemError');
    case 'Skipped':
      return t('result.skipped');
    default:
      return verdict;
  }
}

function getVerdictVariant(verdict: Verdict): BadgeVariant {
  switch (verdict) {
    case 'Accepted':
      return 'accepted';
    case 'WrongAnswer':
      return 'wronganswer';
    case 'TimeLimitExceeded':
      return 'timelimitexceeded';
    case 'MemoryLimitExceeded':
      return 'memorylimitexceeded';
    case 'RuntimeError':
      return 'runtimeerror';
    case 'SystemError':
    case 'Skipped':
      return 'secondary';
    default:
      return 'outline';
  }
}

export function getVerdictBadge(
  verdict: Verdict | null | undefined,
  status: SubmissionStatus,
  t: (key: string) => string,
): {
  label: string;
  variant: BadgeVariant;
} {
  if (status === 'Pending' || status === 'Compiling' || status === 'Running') {
    return { label: getStatusLabel(status, t), variant: 'outline' };
  }
  if (status === 'CompilationError') {
    return { label: t('result.compilationError'), variant: 'secondary' };
  }
  if (status === 'SystemError') {
    return { label: t('result.systemError'), variant: 'secondary' };
  }
  if (!verdict) {
    return { label: getStatusLabel(status, t), variant: 'outline' };
  }
  return {
    label: getVerdictLabel(verdict, t),
    variant: getVerdictVariant(verdict),
  };
}
