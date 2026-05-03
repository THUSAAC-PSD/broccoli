import { useAuth } from '@broccoli/web-sdk/auth';
import { useTranslation } from '@broccoli/web-sdk/i18n';
import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
  Skeleton,
} from '@broccoli/web-sdk/ui';
import { AlertCircle, Inbox } from 'lucide-react';
import { useState } from 'react';

import { PageLayout } from '@/components/PageLayout';
import { Unauthorized } from '@/components/Unauthorized';
import { DlqMessageDetailDialog } from '@/features/dlq/components/DlqMessageDetailDialog';
import { DlqMessageList } from '@/features/dlq/components/DlqMessageList';
import { DlqStatsSummary } from '@/features/dlq/components/DlqStatsSummary';
import { useDlqList } from '@/features/dlq/hooks/useDlqList';
import { useDlqStats } from '@/features/dlq/hooks/useDlqStats';
import type { DlqResolvedFilter } from '@/features/dlq/types';

const PER_PAGE = 25;

const FILTER_OPTIONS: DlqResolvedFilter[] = ['unresolved', 'all', 'resolved'];

export default function AdminDlqPage() {
  const { t } = useTranslation();
  const { user } = useAuth();

  const [page, setPage] = useState(1);
  const [resolvedFilter, setResolvedFilter] =
    useState<DlqResolvedFilter>('unresolved');
  const [selectedId, setSelectedId] = useState<number | null>(null);

  const stats = useDlqStats();
  const list = useDlqList({ page, perPage: PER_PAGE, resolvedFilter });

  if (!user || !user.permissions.includes('dlq:manage')) {
    return <Unauthorized />;
  }

  return (
    <PageLayout
      pageId="admin-dlq"
      title={t('dlq.title')}
      subtitle={t('dlq.subtitle')}
      icon={<Inbox className="h-6 w-6 text-primary" />}
    >
      {stats.error && (
        <Card className="border-destructive">
          <CardContent className="pt-6 flex items-start gap-3">
            <AlertCircle className="h-5 w-5 text-destructive shrink-0" />
            <p className="text-sm text-destructive">{t('dlq.error.stats')}</p>
          </CardContent>
        </Card>
      )}

      {stats.isLoading && !stats.data && (
        <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
          {['unresolved', 'resolved', 'op', 'stuck'].map((k) => (
            <Skeleton key={k} className="h-24 w-full" />
          ))}
        </div>
      )}

      {stats.data && <DlqStatsSummary stats={stats.data} />}

      <Card className="mt-6">
        <CardHeader className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
          <CardTitle className="text-base">{t('dlq.list.title')}</CardTitle>
          <div className="flex gap-1 rounded-md border bg-muted/30 p-0.5 text-xs">
            {FILTER_OPTIONS.map((opt) => (
              <button
                key={opt}
                onClick={() => {
                  setPage(1);
                  setResolvedFilter(opt);
                }}
                className={`rounded px-3 py-1 transition-colors ${
                  resolvedFilter === opt
                    ? 'bg-background font-medium text-foreground shadow-sm'
                    : 'text-muted-foreground hover:text-foreground'
                }`}
              >
                {t(`dlq.filter.${opt}`)}
              </button>
            ))}
          </div>
        </CardHeader>
        <CardContent>
          {list.error && (
            <div className="flex items-start gap-3 rounded-md border border-destructive/40 bg-destructive/5 p-3 text-sm">
              <AlertCircle className="h-4 w-4 shrink-0 text-destructive" />
              <p>{t('dlq.error.list')}</p>
            </div>
          )}

          {list.isLoading && !list.data && (
            <div className="space-y-2">
              {['s1', 's2', 's3', 's4', 's5'].map((k) => (
                <Skeleton key={k} className="h-12 w-full" />
              ))}
            </div>
          )}

          {list.data && (
            <DlqMessageList
              data={list.data}
              onSelect={setSelectedId}
              onPageChange={setPage}
            />
          )}
        </CardContent>
      </Card>

      <DlqMessageDetailDialog
        messageId={selectedId}
        onOpenChange={(open) => {
          if (!open) setSelectedId(null);
        }}
      />
    </PageLayout>
  );
}
