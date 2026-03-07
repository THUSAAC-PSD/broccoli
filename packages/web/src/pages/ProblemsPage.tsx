import { useTranslation } from '@broccoli/sdk/i18n';
import { Slot } from '@broccoli/sdk/react';
import { Code2 } from 'lucide-react';

import { PageLayout } from '@/components/PageLayout';
import { Unauthorized } from '@/components/Unauthorized';
import { useAuth } from '@/contexts/auth-context';
import { AdminProblemsTab } from '@/pages/admin/AdminProblemsTab';

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
    <PageLayout
      pageId="problems"
      title={title}
      icon={<Code2 className="h-6 w-6 text-primary" />}
    >
      <Slot name="problem-list.toolbar" as="div" />

      <AdminProblemsTab contestId={contestId} />
    </PageLayout>
  );
}
