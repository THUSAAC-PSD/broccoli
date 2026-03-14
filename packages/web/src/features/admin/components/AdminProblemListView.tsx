import { useTranslation } from '@broccoli/web-sdk/i18n';
import { Code2 } from 'lucide-react';

import { PageLayout } from '@/components/PageLayout';
import { Unauthorized } from '@/components/Unauthorized';
import { AdminProblemsTab } from '@/features/admin/components/AdminProblemsTab';
import { useAuth } from '@/features/auth/hooks/use-auth';

export function AdminProblemListView({ contestId }: { contestId?: number }) {
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
    <PageLayout
      pageId="problems"
      title={title}
      icon={<Code2 className="h-6 w-6 text-primary" />}
    >
      <AdminProblemsTab contestId={contestId} />
    </PageLayout>
  );
}
