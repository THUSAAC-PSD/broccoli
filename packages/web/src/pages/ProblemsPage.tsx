import { useTranslation } from '@broccoli/sdk/i18n';
import { Slot } from '@broccoli/sdk/react';
import { Code2, Shield } from 'lucide-react';

import { Card, CardContent } from '@/components/ui/card';
import { useAuth } from '@/contexts/auth-context';

import { AdminProblemsTab } from './admin/AdminProblemsTab';

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
        <Code2 className="h-6 w-6 text-primary" />
        <h1 className="text-2xl font-bold">{title}</h1>
      </div>

      <Slot name="problem-list.toolbar" as="div" />

      <AdminProblemsTab contestId={contestId} />
    </div>
  );
}
