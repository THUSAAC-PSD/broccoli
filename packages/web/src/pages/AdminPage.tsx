import { useTranslation } from '@broccoli/web-sdk/i18n';
import { Code2, Trophy } from 'lucide-react';

import { PageLayout } from '@/components/PageLayout';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { Unauthorized } from '@/components/Unauthorized';
import { useAuth } from '@/contexts/auth-context';
import { AdminContestsTab } from '@/pages/admin/AdminContestsTab';
import { AdminProblemsTab } from '@/pages/admin/AdminProblemsTab';

export function AdminPage() {
  const { t } = useTranslation();
  const { user } = useAuth();

  if (!user || !user.permissions.includes('contest:create')) {
    return <Unauthorized />;
  }

  return (
    <PageLayout
      pageId="admin"
      title={t('admin.title')}
      subtitle={t('admin.subtitle')}
    >
      <Tabs defaultValue="contests">
        <TabsList className="grid w-full grid-cols-2 max-w-md">
          <TabsTrigger value="contests" className="gap-2">
            <Trophy className="h-4 w-4" />
            {t('admin.contests')}
          </TabsTrigger>
          <TabsTrigger value="problems" className="gap-2">
            <Code2 className="h-4 w-4" />
            {t('admin.problems')}
          </TabsTrigger>
        </TabsList>

        <TabsContent value="contests" className="mt-4">
          <AdminContestsTab />
        </TabsContent>

        <TabsContent value="problems" className="mt-4">
          <AdminProblemsTab />
        </TabsContent>
      </Tabs>
    </PageLayout>
  );
}
