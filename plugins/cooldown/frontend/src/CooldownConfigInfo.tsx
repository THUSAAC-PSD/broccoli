/**
 * Displays config inheritance info above the config form fields.
 */
import { useApiFetch } from '@broccoli/web-sdk/api';
import { useTranslation } from '@broccoli/web-sdk/i18n';
import { cn } from '@broccoli/web-sdk/utils';
import { useEffect, useState } from 'react';

type ConfigScope =
  | { scope: 'plugin'; pluginId: string }
  | { scope: 'contest'; contestId: number }
  | { scope: 'problem'; problemId: number }
  | { scope: 'contest_problem'; contestId: number; problemId: number };

interface JsonSchema {
  properties?: Record<string, { default?: unknown }>;
}

interface Props {
  scope?: ConfigScope;
  pluginId?: string;
  namespace?: string;
  schema?: JsonSchema;
  hasExplicitValue?: (path: string | string[]) => boolean;
}

interface ParentValues {
  contest: number | null;
  problem: number | null;
}

interface Effective {
  value: number;
  source: string;
}

function formatCooldown(
  v: number,
  t: (key: string, params?: Record<string, string | number>) => string,
): string {
  return v === 0
    ? t('cooldown.disabled')
    : t('cooldown.secondsValue', { seconds: v });
}

/** Resolve the effective inherited value. Contest \> Problem. */
function resolveEffective(
  values: ParentValues,
  t: (key: string) => string,
): Effective | null {
  if (values.contest !== null)
    return { value: values.contest, source: t('cooldown.contest') };
  if (values.problem !== null)
    return { value: values.problem, source: t('cooldown.problem') };
  return null;
}

export function CooldownConfigInfo({
  scope,
  pluginId,
  namespace,
  schema,
  hasExplicitValue,
}: Props) {
  const apiFetch = useApiFetch();
  const { t } = useTranslation();
  const [parentValues, setParentValues] = useState<ParentValues | null>(null);

  const isContestProblem = scope?.scope === 'contest_problem';
  const isContest = scope?.scope === 'contest';

  useEffect(() => {
    if (!pluginId || !namespace || !isContestProblem) return;

    let cancelled = false;
    const s = scope as { contestId: number; problemId: number };
    const fetchConfig = async (
      path: string,
    ): Promise<Record<string, string | number> | null> => {
      try {
        const res = await apiFetch(`/api/v1${path}`);
        if (!res.ok) return null;
        const data = await res.json();
        return (data?.config ?? null) as Record<string, string | number> | null;
      } catch {
        return null;
      }
    };
    Promise.all([
      fetchConfig(`/contests/${s.contestId}/config/${pluginId}/${namespace}`),
      fetchConfig(`/problems/${s.problemId}/config/${pluginId}/${namespace}`),
    ]).then(([contestCfg, problemCfg]) => {
      if (cancelled) return;
      setParentValues({
        contest: extractCooldown(contestCfg),
        problem: extractCooldown(problemCfg),
      });
    });

    return () => {
      cancelled = true;
    };
  }, [apiFetch, scope, pluginId, namespace, isContestProblem]);

  const schemaDefault = schema?.properties?.cooldown_seconds?.default;
  const defaultLabel =
    typeof schemaDefault === 'number' ? formatCooldown(schemaDefault, t) : null;

  if (!isContestProblem && !isContest) return null;

  // Contest scope: simple hint
  if (isContest) {
    return (
      <p className="m-0 text-xs text-muted-foreground">
        {t('cooldown.contestScopeHint')}
      </p>
    );
  }

  // Contest-problem scope
  const effective = parentValues ? resolveEffective(parentValues, t) : null;
  const hasLocalOverride = hasExplicitValue?.('cooldown_seconds') ?? false;

  // Rows: Contest then Problem (explicit priority order)
  const rows: { label: string; value: number | null }[] = parentValues
    ? [
        { label: t('cooldown.contest'), value: parentValues.contest },
        { label: t('cooldown.problem'), value: parentValues.problem },
      ]
    : [];

  return (
    <div
      className={cn(
        'text-xs text-muted-foreground flex flex-col',
        rows.length > 0 ? 'gap-1.5' : 'gap-0',
      )}
    >
      <p className="m-0">
        {!hasLocalOverride && effective
          ? t('cooldown.inheritInfo', {
              value: formatCooldown(effective.value, t),
              source: effective.source,
            })
          : t('cooldown.overrideInfo')}
      </p>
      {rows.length > 0 && (
        <div className="flex flex-col gap-0.5">
          {rows.map((row) => (
            <div key={row.label} className="flex gap-1.5 items-baseline">
              <span className="font-medium min-w-[52px]">{row.label}:</span>
              {row.value === null ? (
                <span className="italic text-[11px]">
                  {defaultLabel
                    ? t('cooldown.notSetWithDefault', { default: defaultLabel })
                    : t('cooldown.notSet')}
                </span>
              ) : (
                <code className="bg-muted px-1.5 rounded text-[11px]">
                  {formatCooldown(row.value, t)}
                </code>
              )}
              {!hasLocalOverride && row.label === effective?.source && (
                <span className="text-[11px] text-primary">
                  ← {t('cooldown.active')}
                </span>
              )}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

function extractCooldown(
  config: Record<string, string | number> | null,
): number | null {
  if (!config) return null;
  const v = config.cooldown_seconds;
  return typeof v === 'number' ? v : null;
}
