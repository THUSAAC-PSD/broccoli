import { ChevronLeft, ChevronRight } from 'lucide-react';
import { Fragment, type ReactNode, useState } from 'react';

import { useTranslation } from '@/i18n';
import { Button } from '@/ui/button';

/** Render one page of a local or server-paged list, retaining original indices. */
export function PaginatedList<T>({
  items,
  pageSize = 20,
  children,
  onPageChange,
  pagination,
  loading = false,
}: {
  items: readonly T[];
  pageSize?: number;
  children: (items: readonly T[], offset: number) => ReactNode;
  onPageChange?: (page: number) => void;
  pagination?: {
    page: number;
    per_page: number;
    total: number;
    total_pages: number;
  } | null;
  loading?: boolean;
}) {
  const { t } = useTranslation();
  const [requestedPage, setPage] = useState(0);
  const size = pagination?.per_page ?? Math.max(1, Math.floor(pageSize));
  const total = pagination?.total ?? items.length;
  const pageCount = Math.max(1, Math.ceil(total / size));
  const page = pagination
    ? pagination.page - 1
    : Math.min(requestedPage, pageCount - 1);
  const offset = page * size;

  function changePage(next: number) {
    setPage(next);
    onPageChange?.(next + 1);
  }

  return (
    <div className="space-y-2">
      <Fragment key={page}>
        {children(
          pagination ? items : items.slice(offset, offset + size),
          offset,
        )}
      </Fragment>
      {pageCount > 1 && (
        <nav
          aria-label={t('list.pagination')}
          className="flex flex-wrap items-center justify-between gap-2 py-1"
        >
          <span className="text-xs text-muted-foreground" aria-live="polite">
            {t('list.range', {
              start: offset + 1,
              end: Math.min(offset + size, total),
              total,
            })}
          </span>
          <div className="flex items-center gap-2">
            <Button
              type="button"
              variant="outline"
              size="sm"
              disabled={loading || page === 0}
              onClick={() => changePage(page - 1)}
            >
              <ChevronLeft className="h-4 w-4" />
              {t('list.previous')}
            </Button>
            <span className="text-xs tabular-nums text-muted-foreground">
              {page + 1} / {pageCount}
            </span>
            <Button
              type="button"
              variant="outline"
              size="sm"
              disabled={loading || page === pageCount - 1}
              onClick={() => changePage(page + 1)}
            >
              {t('list.next')}
              <ChevronRight className="h-4 w-4" />
            </Button>
          </div>
        </nav>
      )}
    </div>
  );
}
