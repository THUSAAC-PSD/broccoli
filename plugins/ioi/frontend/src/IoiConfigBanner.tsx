import { useTranslation } from '@broccoli/web-sdk/i18n';
import { useQuery } from '@tanstack/react-query';

import { useIoiApi } from './hooks/useIoiApi';

interface IoiConfigBannerProps {
  scope?: {
    scope?: string;
    contestId?: number;
    contest_id?: number;
  };
}

export function IoiConfigBanner({ scope }: IoiConfigBannerProps) {
  const { t } = useTranslation();
  const api = useIoiApi();

  const contestId = scope?.contestId ?? scope?.contest_id;
  const isContestScope =
    scope?.scope === 'contest' || scope?.scope === 'contest_problem';

  const { data: scoreboard } = useQuery({
    queryKey: ['ioi-scoreboard', contestId],
    enabled: !!contestId && isContestScope,
    queryFn: () => api.getScoreboard(contestId!),
    staleTime: 60000,
    retry: false,
  });

  if (!isContestScope || !contestId) return null;

  const phase = scoreboard?.phase;

  if (phase === 'during') {
    return (
      <div
        style={{
          padding: '10px 14px',
          borderRadius: 6,
          marginBottom: 12,
          fontSize: 13,
          fontWeight: 500,
          background: 'rgba(245, 158, 11, 0.08)',
          color: '#b45309',
          border: '1px solid rgba(245, 158, 11, 0.2)',
          display: 'flex',
          alignItems: 'center',
          gap: 8,
        }}
      >
        <span style={{ fontSize: 16 }}>!</span>
        {t('ioi.config.activeContestWarning')}
      </div>
    );
  }

  if (phase === 'before') {
    return (
      <div
        style={{
          padding: '8px 14px',
          borderRadius: 6,
          marginBottom: 12,
          fontSize: 12,
          color: 'var(--muted-foreground, #888)',
          background: 'var(--muted, #f9fafb)',
          border: '1px solid var(--border, #e5e7eb)',
        }}
      >
        {t('ioi.config.beforeContestInfo')}
      </div>
    );
  }

  return null;
}
