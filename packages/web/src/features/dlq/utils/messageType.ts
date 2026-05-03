import { Cog, FileText, type LucideIcon } from 'lucide-react';

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
  return { labelKey: type, icon: Cog, retryable: false };
}
