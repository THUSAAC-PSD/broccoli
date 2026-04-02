/**
 * Config inheritance display for the cooldown plugin.
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

export function CooldownConfigInfo(props: Props) {
  const { t } = useTranslation();

  return (
    <ConfigInheritanceInfo
      {...props}
      scope={
        props.scope as Parameters<typeof ConfigInheritanceInfo>[0]['scope']
      }
      fieldKey="cooldown_seconds"
      formatValue={(v) =>
        v === 0
          ? t('cooldown.disabled')
          : t('cooldown.secondsValue', { seconds: v as number })
      }
      labels={{
        contest: t('cooldown.contest'),
        problem: t('cooldown.problem'),
        contestHint: t('cooldown.contestScopeHint'),
        inheritInfo: (value, source) =>
          t('cooldown.inheritInfo', { value, source }),
        overrideInfo: t('cooldown.overrideInfo'),
        notSet: t('cooldown.notSet'),
        notSetWithDefault: (def) =>
          t('cooldown.notSetWithDefault', { default: def }),
        active: t('cooldown.active'),
      }}
    />
  );
}
