import { useTranslation } from '@broccoli/sdk/i18n';
import { Slot } from '@broccoli/sdk/react';
import { Code2 } from 'lucide-react';

import { Unauthorized } from '@/components/Unauthorized';
import { useAuth } from '@/contexts/auth-context';
import { AdminProblemsTab } from '@/pages/admin/AdminProblemsTab';

// --- Page ---

export function ProblemsPage({ contestId }: { contestId?: number }) {
  const { t } = useTranslation();
  const { user } = useAuth();

  const title = contestId ? t('problems.contestProblems') : t('problems.title');

  if (
    !user ||
    (!user.permissions.includes('problem:create') &&
      !user.permissions.includes('problem:edit'))
  ) {
    return <Unauthorized />;
  }

  return (
    <div className="flex flex-col gap-4 p-6">
      <div className="flex items-center gap-3">
        <Code2 className="h-6 w-6 text-primary" />
        <h1 className="text-2xl font-bold">{title}</h1>
      </div>

      <Slot name="problem-list.toolbar" as="div" />

      <AdminProblemsTab contestId={contestId} />
    </div>
  );
}
