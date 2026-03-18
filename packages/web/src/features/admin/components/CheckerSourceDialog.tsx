import { useTranslation } from '@broccoli/web-sdk/i18n';
import type { ProblemSummary } from '@broccoli/web-sdk/problem';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from '@broccoli/web-sdk/ui';

import { CheckerSourceSection } from '@/features/admin/components/CheckerSourceSection';

interface CheckerSourceDialogProps {
  problem: ProblemSummary;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

export function CheckerSourceDialog({
  problem,
  open,
  onOpenChange,
}: CheckerSourceDialogProps) {
  const { t } = useTranslation();

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-3xl max-h-[90vh] flex flex-col">
        <DialogHeader>
          <DialogTitle>{t('admin.checkerSource.title')}</DialogTitle>
          <DialogDescription>{problem.title}</DialogDescription>
        </DialogHeader>
        <div className="overflow-y-auto flex-1">
          <CheckerSourceSection problemId={problem.id} />
        </div>
      </DialogContent>
    </Dialog>
  );
}
