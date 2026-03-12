import { useTranslation } from '@broccoli/web-sdk/i18n';
import { Button } from '@broccoli/web-sdk/ui';

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
        <p className="text-[10px] font-semibold uppercase tracking-[0.15em] text-muted-foreground">
          {t('contests.enrollTitle')}
        </p>
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
