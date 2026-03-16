import { useTranslation } from '@broccoli/web-sdk/i18n';
import type { ProblemSummary } from '@broccoli/web-sdk/problem';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from '@broccoli/web-sdk/ui';

import { AttachmentsSection } from '@/features/admin/components/AttachmentsSection';

interface AttachmentsDialogProps {
  problem: ProblemSummary;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

export function AttachmentsDialog({
  problem,
  open,
  onOpenChange,
}: AttachmentsDialogProps) {
  const { t } = useTranslation();

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-3xl max-h-[90vh] flex flex-col">
        <DialogHeader>
          <DialogTitle>{t('admin.attachments.title')}</DialogTitle>
          <DialogDescription>{problem.title}</DialogDescription>
        </DialogHeader>
        <div className="overflow-y-auto flex-1">
          <AttachmentsSection problemId={problem.id} />
        </div>
      </DialogContent>
    </Dialog>
  );
}
