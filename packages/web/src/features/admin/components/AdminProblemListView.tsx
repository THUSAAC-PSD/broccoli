import { useAuth } from '@broccoli/web-sdk/auth';
import { useTranslation } from '@broccoli/web-sdk/i18n';
import { PROBLEM_CREATE, PROBLEM_EDIT } from '@broccoli/web-sdk/permissions';
import { Code2 } from 'lucide-react';

import { PageLayout } from '@/components/PageLayout';
import { Unauthorized } from '@/components/Unauthorized';
import { AdminProblemsTab } from '@/features/admin/components/AdminProblemsTab';

export function AdminProblemListView({ contestId }: { contestId?: number }) {
  const { t } = useTranslation();
  const { user } = useAuth();

  const title = contestId ? t('problems.contestProblems') : t('problems.title');

  if (
    !user ||
    (!user.permissions.includes(PROBLEM_CREATE) &&
      !user.permissions.includes(PROBLEM_EDIT))
  ) {
    return <Unauthorized />;
  }

  return (
    <PageLayout
      pageId="problems"
      title={title}
      icon={<Code2 className="h-6 w-6 text-primary" />}
    >
      <AdminProblemsTab contestId={contestId} />
    </PageLayout>
  );
}
