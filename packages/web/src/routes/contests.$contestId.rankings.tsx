import { useTranslation } from '@broccoli/web-sdk/i18n';
import { Slot } from '@broccoli/web-sdk/slot';
import { BarChart3 } from 'lucide-react';

import { PageLayout } from '@/components/PageLayout';

export default function ContestRankingPage() {
  const { t } = useTranslation();

  return (
    <PageLayout
      pageId="ranking"
      icon={<BarChart3 className="h-6 w-6 text-sidebar-primary" />}
      title={t('ranking.title')}
      contentClassName="flex flex-col gap-6"
    >
      <Slot name="ranking.header" as="div" />
      <Slot name="ranking.content" as="div" className="w-full">
        <div className="rounded-lg border border-dashed p-8 text-center text-sm text-muted-foreground">
          {t('ranking.empty')}
        </div>
      </Slot>
    </PageLayout>
  );
}
