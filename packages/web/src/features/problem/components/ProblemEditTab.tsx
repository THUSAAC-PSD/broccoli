import { useTranslation } from '@broccoli/web-sdk/i18n';
import { Button } from '@broccoli/web-sdk/ui';
import { ArrowLeft } from 'lucide-react';

import { ProblemEditForm } from './ProblemEditForm';

interface ProblemEditTabProps {
  problemId: number;
  onBack: () => void;
}

export function ProblemEditTab({ problemId, onBack }: ProblemEditTabProps) {
  const { t } = useTranslation();

  return (
    <>
      <div className="flex shrink-0 items-center justify-between border-b bg-background px-6 py-1.5">
        <Button
          variant="ghost"
          size="sm"
          onClick={onBack}
          className="-ml-2 h-8 gap-1.5 text-sm font-semibold text-foreground hover:text-foreground"
        >
          <ArrowLeft className="h-3.5 w-3.5" />
          {t('problem.backToDescription')}
        </Button>
        <div className="h-8" />
      </div>
      <div className="flex-1 overflow-y-auto p-6">
        <div className="mx-auto max-w-4xl">
          <ProblemEditForm problemId={problemId} />
        </div>
      </div>
    </>
  );
}
