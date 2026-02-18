import { useTranslation } from '@broccoli/sdk/i18n';
import { Code2, Shield, Trophy } from 'lucide-react';

import { useAuth } from '@/contexts/auth-context';
import { Card, CardContent } from '@/components/ui/card';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';

import { AdminContestsTab } from './admin/AdminContestsTab';
import { AdminProblemsTab } from './admin/AdminProblemsTab';

export function AdminPage() {
  const { t } = useTranslation();
  const { user } = useAuth();

  if (
    !user ||
    (user.role !== 'admin' && !user.permissions.includes('contest:create'))
  ) {
    return (
      <div className="flex items-center justify-center h-full">
        <Card className="max-w-md">
          <CardContent className="pt-6 text-center">
            <Shield className="mx-auto h-12 w-12 text-muted-foreground mb-4" />
            <p className="text-destructive text-lg font-medium">
              {t('admin.unauthorized')}
            </p>
          </CardContent>
        </Card>
      </div>
    );
  }

  return (
    <div className="mx-auto max-w-5xl p-6 space-y-6">
      <div>
        <h1 className="text-2xl font-bold tracking-tight">
          {t('admin.title')}
        </h1>
        <p className="text-muted-foreground">{t('admin.subtitle')}</p>
      </div>

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
    </div>
  );
}
