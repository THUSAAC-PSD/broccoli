import { useAuth } from '@broccoli/web-sdk/auth';
import { useTranslation } from '@broccoli/web-sdk/i18n';
import { Activity } from 'lucide-react';

import { PageLayout } from '@/components/PageLayout';
import { Unauthorized } from '@/components/Unauthorized';
import { AllSubmissions } from '@/features/submission/components/AllSubmissions';

export default function AdminSubmissionsPage() {
  const { t } = useTranslation();
  const { user } = useAuth();

  if (!user || !user.permissions.includes('submission:view_all')) {
    return <Unauthorized />;
  }

  return (
    <PageLayout
      pageId="admin-submissions"
      title={t('adminSubmissions.title')}
      subtitle={t('adminSubmissions.subtitle')}
      icon={<Activity className="h-6 w-6 text-primary" />}
    >
      <AllSubmissions />
    </PageLayout>
  );
}
