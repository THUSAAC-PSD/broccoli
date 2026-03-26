import { useAuth } from '@broccoli/web-sdk/auth';
import { useTranslation } from '@broccoli/web-sdk/i18n';
import { Trophy } from 'lucide-react';

import { PageLayout } from '@/components/PageLayout';
import { Unauthorized } from '@/components/Unauthorized';
import { AdminContestsTab } from '@/features/admin/components/AdminContestsTab';

export default function ContestListPage() {
  const { t } = useTranslation();
  const { user } = useAuth();

  if (!user || !user.permissions.includes('contest:manage')) {
    return <Unauthorized />;
  }

  return (
    <PageLayout
      pageId="contests"
      title={t('contests.title')}
      icon={<Trophy className="h-6 w-6 text-primary" />}
    >
      <AdminContestsTab />
    </PageLayout>
  );
}
