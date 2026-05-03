import { useTranslation } from '@broccoli/web-sdk/i18n';
import { Server } from 'lucide-react';

import { WorkerCard } from '@/features/system/components/WorkerCard';
import type { WorkerInfo } from '@/features/system/types';

interface Props {
  workers: WorkerInfo[];
}

export function WorkersGrid({ workers }: Props) {
  const { t } = useTranslation();

  if (workers.length === 0) {
    return (
      <div className="rounded-lg border border-dashed bg-muted/20 p-8 text-center">
        <Server className="mx-auto mb-3 h-8 w-8 text-muted-foreground" />
        <p className="text-sm font-medium">{t('system.workers.empty.title')}</p>
        <p className="mt-1 text-xs text-muted-foreground">
          {t('system.workers.empty.hint')}
        </p>
      </div>
    );
  }

  return (
    <div className="grid gap-3 sm:grid-cols-2">
      {workers.map((w) => (
        <WorkerCard key={w.id} worker={w} />
      ))}
    </div>
  );
}
