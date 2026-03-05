import { useTranslation } from '@broccoli/sdk/i18n';
import { Trophy } from 'lucide-react';

import { AdminContestsTab } from './admin/AdminContestsTab';

export function ContestsPage() {
  const { t } = useTranslation();

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
