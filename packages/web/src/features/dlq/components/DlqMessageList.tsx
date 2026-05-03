import { useTranslation } from '@broccoli/web-sdk/i18n';
import { Button } from '@broccoli/web-sdk/ui';
import { Inbox } from 'lucide-react';

import { DlqMessageRow } from '@/features/dlq/components/DlqMessageRow';
import type { DlqListResponse } from '@/features/dlq/types';

interface Props {
  data: DlqListResponse;
  onSelect: (id: number) => void;
  onPageChange: (page: number) => void;
}

export function DlqMessageList({ data, onSelect, onPageChange }: Props) {
  const { t } = useTranslation();
  const { data: messages, pagination } = data;

  if (messages.length === 0) {
    return (
      <div className="rounded-lg border border-dashed bg-muted/20 p-10 text-center">
        <Inbox className="mx-auto mb-3 h-8 w-8 text-muted-foreground" />
        <p className="text-sm font-medium">{t('dlq.list.empty.title')}</p>
        <p className="mt-1 text-xs text-muted-foreground">
          {t('dlq.list.empty.hint')}
        </p>
      </div>
    );
  }

  return (
    <div className="space-y-3">
      <div className="overflow-x-auto rounded-lg border bg-card">
        <table className="w-full border-collapse text-sm">
          <thead>
            <tr className="border-b bg-muted/30 text-left text-xs uppercase tracking-wide text-muted-foreground">
              <th className="p-3 font-medium">{t('dlq.col.messageId')}</th>
              <th className="p-3 font-medium">{t('dlq.col.type')}</th>
              <th className="p-3 font-medium">{t('dlq.col.error')}</th>
              <th className="p-3 text-center font-medium">
                {t('dlq.col.retries')}
              </th>
              <th className="p-3 font-medium">{t('dlq.col.failedAt')}</th>
              <th className="p-3 font-medium">{t('dlq.col.status')}</th>
            </tr>
          </thead>
          <tbody>
            {messages.map((m) => (
              <DlqMessageRow key={m.id} message={m} onClick={onSelect} />
            ))}
          </tbody>
        </table>
      </div>

      {pagination.total_pages > 1 && (
        <div className="flex items-center justify-between text-sm">
          <span className="text-muted-foreground tabular-nums">
            {t('dlq.list.pageOf', {
              page: pagination.page,
              total: pagination.total_pages,
              count: pagination.total,
            })}
          </span>
          <div className="flex gap-2">
            <Button
              variant="outline"
              size="sm"
              disabled={pagination.page <= 1}
              onClick={() => onPageChange(pagination.page - 1)}
            >
              {t('dlq.list.prev')}
            </Button>
            <Button
              variant="outline"
              size="sm"
              disabled={pagination.page >= pagination.total_pages}
              onClick={() => onPageChange(pagination.page + 1)}
            >
              {t('dlq.list.next')}
            </Button>
          </div>
        </div>
      )}
    </div>
  );
}
