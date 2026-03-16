import { useTranslation } from '@broccoli/web-sdk/i18n';
import type { ProblemSummary } from '@broccoli/web-sdk/problem';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from '@broccoli/web-sdk/ui';

import { AdditionalFilesSection } from '@/features/admin/components/AdditionalFilesSection';

interface AdditionalFilesDialogProps {
  problem: ProblemSummary;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

export function AdditionalFilesDialog({
  problem,
  open,
  onOpenChange,
}: AdditionalFilesDialogProps) {
  const { t } = useTranslation();

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-3xl max-h-[90vh] flex flex-col">
        <DialogHeader>
          <DialogTitle>{t('admin.additionalFiles.title')}</DialogTitle>
          <DialogDescription>{problem.title}</DialogDescription>
        </DialogHeader>
        <div className="overflow-y-auto flex-1">
          <AdditionalFilesSection problemId={problem.id} />
        </div>
      </DialogContent>
    </Dialog>
  );
}
