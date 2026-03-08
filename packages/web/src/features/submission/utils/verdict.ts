import type { SubmissionStatus, Verdict } from '@broccoli/web-sdk';

export function getVerdictBadge(
  verdict: Verdict | null | undefined,
  status: SubmissionStatus,
): {
  label: string;
  variant: 'default' | 'secondary' | 'destructive' | 'outline';
} {
  if (status === 'Pending' || status === 'Compiling' || status === 'Running') {
    return { label: status, variant: 'outline' };
  }
  if (status === 'CompilationError') {
    return { label: 'CE', variant: 'secondary' };
  }
  if (status === 'SystemError') {
    return { label: 'SE', variant: 'secondary' };
  }
  if (!verdict) {
    return { label: status, variant: 'outline' };
  }
  if (verdict === 'Accepted') {
    return { label: 'AC', variant: 'default' };
  }
  const shortNames: Record<string, string> = {
    WrongAnswer: 'WA',
    TimeLimitExceeded: 'TLE',
    MemoryLimitExceeded: 'MLE',
    RuntimeError: 'RE',
    SystemError: 'SE',
  };
  return { label: shortNames[verdict] ?? verdict, variant: 'destructive' };
}
