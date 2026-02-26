import { useTranslation } from '@broccoli/sdk/i18n';
import { Slot } from '@broccoli/sdk/react';
import { Code2 } from 'lucide-react';

import { AdminProblemsTab } from './admin/AdminProblemsTab';

// --- Page ---

export function ProblemsPage({ contestId }: { contestId?: number }) {
  const { t } = useTranslation();

  const title = contestId ? t('problems.contestProblems') : t('problems.title');

  return (
    <div className="flex flex-col gap-6 p-6">
      <div className="flex items-center gap-3">
        <Code2 className="h-6 w-6 text-primary" />
        <h1 className="text-2xl font-bold">{title}</h1>
      </div>

      <Slot name="problem-list.toolbar" as="div" />

      <AdminProblemsTab contestId={contestId} />
    </div>
  );
}
