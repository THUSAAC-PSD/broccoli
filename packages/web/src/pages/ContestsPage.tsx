import { useTranslation } from '@broccoli/sdk/i18n';
import { Shield, Trophy } from 'lucide-react';

import { Card, CardContent } from '@/components/ui/card';
import { useAuth } from '@/contexts/auth-context';

import { AdminContestsTab } from './admin/AdminContestsTab';

export function ContestsPage() {
  const { t } = useTranslation();
  const { user } = useAuth();

  if (
    !user ||
    !user.permissions.includes('contest:create') ||
    !user.permissions.includes('contest:edit')
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
    <div className="flex flex-col gap-4 p-6">
      <div className="flex items-center gap-3">
        <Trophy className="h-6 w-6 text-primary" />
        <h1 className="text-2xl font-bold">{t('contests.title')}</h1>
      </div>

      <AdminContestsTab />
    </div>
  );
}
