import { useTranslation } from '@broccoli/web-sdk/i18n';
import { Button } from '@broccoli/web-sdk/ui';

const ACCENT = 'hsl(var(--sidebar-ring))';

export function ContestEnrollCard({
  onEnroll,
  isPending,
}: {
  onEnroll: () => void;
  isPending: boolean;
}) {
  const { t } = useTranslation();

  return (
    <div className="rounded-lg border p-4 space-y-3">
      <div>
        <div className="inline-flex items-center gap-1.5 px-2">
          <span
            className={`h-1.5 w-1.5 shrink-0 rounded-full`}
            style={{ backgroundColor: ACCENT }}
          />
          <span
            className="text-[10px] font-semibold uppercase tracking-[0.15em]"
            style={{ color: ACCENT }}
          >
            {t('contests.enrollTitle')}
          </span>
        </div>
        <p className="text-sm text-muted-foreground mt-1">
          {t('contests.enrollDescription')}
        </p>
      </div>
      <Button className="w-full" onClick={onEnroll} disabled={isPending}>
        {isPending ? t('contests.enrolling') : t('contests.enrollAction')}
      </Button>
    </div>
  );
}
