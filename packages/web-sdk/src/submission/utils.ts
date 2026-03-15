import type {
  SubmissionStatus,
  SubmissionStatusFilterValue,
} from '@/submission/types';

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
