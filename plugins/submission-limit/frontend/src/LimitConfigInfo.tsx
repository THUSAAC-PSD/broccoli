/**
 * Config inheritance display for the submission-limit plugin.
 */
import { useTranslation } from '@broccoli/web-sdk/i18n';
import { ConfigInheritanceInfo } from '@broccoli/web-sdk/plugin';

interface Props {
  scope?: { scope: string; [key: string]: unknown };
  pluginId?: string;
  namespace?: string;
  schema?: { properties?: Record<string, { default?: unknown }> };
  hasExplicitValue?: (path: string | string[]) => boolean;
  inherited?: {
    contest?: { values: Record<string, unknown> | null; enabled: boolean };
    problem?: { values: Record<string, unknown> | null; enabled: boolean };
  };
}

export function LimitConfigInfo(props: Props) {
  const { t } = useTranslation();

  return (
    <ConfigInheritanceInfo
      {...props}
      scope={
        props.scope as Parameters<typeof ConfigInheritanceInfo>[0]['scope']
      }
      fieldKey="max_submissions"
      formatValue={(v) => (v === 0 ? t('limit.unlimited') : String(v))}
      labels={{
        contest: t('limit.sourceContest'),
        problem: t('limit.sourceProblem'),
        contestHint: t('limit.contestHint'),
        inheritInfo: (value, source) =>
          t('limit.inheritInfo', { value, source }),
        overrideInfo: t('limit.overrideInfo'),
        notSet: t('limit.notSet'),
        notSetWithDefault: (def) =>
          t('limit.notSetWithDefault', { default: def }),
        active: t('limit.active'),
      }}
    />
  );
}
