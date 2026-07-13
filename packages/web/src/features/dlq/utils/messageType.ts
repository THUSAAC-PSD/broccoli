import {
  Cog,
  FileCode,
  FileText,
  History,
  type LucideIcon,
} from 'lucide-react';

export interface MessageTypeMeta {
  labelKey: string;
  icon: LucideIcon;
  retryable: boolean;
}

export function messageTypeMeta(type: string): MessageTypeMeta {
  if (type === 'stuck_submission') {
    return {
      labelKey: 'dlq.type.stuckSubmission',
      icon: FileText,
      retryable: true,
    };
  }
  if (type === 'operation_task') {
    return {
      labelKey: 'dlq.type.operationTask',
      icon: Cog,
      retryable: false,
    };
  }
  if (type === 'stuck_code_run') {
    return {
      labelKey: 'dlq.type.stuckCodeRun',
      icon: FileCode,
      retryable: false,
    };
  }
  if (type === 'stuck_submission_judgement') {
    return {
      labelKey: 'dlq.type.stuckSubmissionJudgement',
      icon: History,
      retryable: false,
    };
  }
  return { labelKey: type, icon: Cog, retryable: false };
}
