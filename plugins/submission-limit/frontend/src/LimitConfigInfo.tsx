/**
 * Displays config inheritance info above the config form fields.
 */
import { useTranslation } from '@broccoli/sdk/i18n';
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
}

interface ParentValues {
  contest: number | null;
  problem: number | null;
}

interface Effective {
  value: number;
  source: string;
}

function formatLimit(v: number, t: (key: string) => string): string {
  return v === 0 ? t('limit.unlimited') : String(v);
}

/** Resolve the effective inherited value. Contest \> Problem. */
function resolveEffective(
  values: ParentValues,
  t: (key: string) => string,
): Effective | null {
  if (values.contest !== null)
    return { value: values.contest, source: t('limit.sourceContest') };
  if (values.problem !== null)
    return { value: values.problem, source: t('limit.sourceProblem') };
  return null;
}

const BACKEND_ORIGIN = new URL(import.meta.url).origin;
const AUTH_TOKEN_KEY = 'broccoli_token';

function authHeaders(): HeadersInit {
  const token = localStorage.getItem(AUTH_TOKEN_KEY);
  return token ? { Authorization: `Bearer ${token}` } : {};
}

async function fetchConfig(
  path: string,
): Promise<Record<string, unknown> | null> {
  try {
    const res = await fetch(`${BACKEND_ORIGIN}/api/v1${path}`, {
      headers: authHeaders(),
    });
    if (!res.ok) return null;
    const data = await res.json();
    return (data?.config ?? null) as Record<string, unknown> | null;
  } catch {
    return null;
  }
}

export function LimitConfigInfo({ scope, pluginId, namespace, schema }: Props) {
  const { t } = useTranslation();
  const [parentValues, setParentValues] = useState<ParentValues | null>(null);

  const isContestProblem = scope?.scope === 'contest_problem';
  const isContest = scope?.scope === 'contest';

  useEffect(() => {
    if (!pluginId || !namespace || !isContestProblem) return;

    let cancelled = false;
    const s = scope as { contestId: number; problemId: number };
    Promise.all([
      fetchConfig(`/contests/${s.contestId}/config/${pluginId}/${namespace}`),
      fetchConfig(`/problems/${s.problemId}/config/${pluginId}/${namespace}`),
    ]).then(([contestCfg, problemCfg]) => {
      if (cancelled) return;
      setParentValues({
        contest: extractMaxSubmissions(contestCfg),
        problem: extractMaxSubmissions(problemCfg),
      });
    });

    return () => {
      cancelled = true;
    };
  }, [scope, pluginId, namespace, isContestProblem]);

  const schemaDefault = schema?.properties?.max_submissions?.default;
  const defaultLabel =
    typeof schemaDefault === 'number' ? formatLimit(schemaDefault, t) : null;

  if (!isContestProblem && !isContest) return null;

  // Contest scope: simple hint
  if (isContest) {
    return (
      <p
        style={{
          margin: 0,
          fontSize: '12px',
          color: 'var(--muted-foreground, #6b7280)',
        }}
      >
        {t('limit.contestHint')}
      </p>
    );
  }

  // Contest-problem scope
  const effective = parentValues ? resolveEffective(parentValues, t) : null;

  // Rows: Contest then Problem (explicit priority order)
  const rows: { label: string; value: number | null }[] = parentValues
    ? [
        { label: t('limit.sourceContest'), value: parentValues.contest },
        { label: t('limit.sourceProblem'), value: parentValues.problem },
      ]
    : [];

  return (
    <div
      style={{
        fontSize: '12px',
        color: 'var(--muted-foreground, #6b7280)',
        display: 'flex',
        flexDirection: 'column',
        gap: rows.length > 0 ? '6px' : '0',
      }}
    >
      <p style={{ margin: 0 }}>
        {effective ? (
          <>
            {t('limit.inheritInfo', {
              value: formatLimit(effective.value, t),
              source: effective.source,
            })}{' '}
            {t('limit.priority')}
          </>
        ) : (
          t('limit.overrideInfo')
        )}
      </p>
      {rows.length > 0 && (
        <div style={{ display: 'flex', flexDirection: 'column', gap: '2px' }}>
          {rows.map((row) => (
            <div
              key={row.label}
              style={{ display: 'flex', gap: '6px', alignItems: 'baseline' }}
            >
              <span style={{ fontWeight: 500, minWidth: '52px' }}>
                {row.label}:
              </span>
              {row.value !== null ? (
                <code
                  style={{
                    background: 'var(--muted, #f3f4f6)',
                    padding: '0 5px',
                    borderRadius: '3px',
                    fontSize: '11px',
                  }}
                >
                  {formatLimit(row.value, t)}
                </code>
              ) : (
                <span style={{ fontStyle: 'italic', fontSize: '11px' }}>
                  {defaultLabel
                    ? t('limit.notSetWithDefault', { default: defaultLabel })
                    : t('limit.notSet')}
                </span>
              )}
              {effective && row.label === effective.source && (
                <span
                  style={{ fontSize: '11px', color: 'var(--primary, #2563eb)' }}
                >
                  ← {t('limit.active')}
                </span>
              )}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

function extractMaxSubmissions(
  config: Record<string, unknown> | null,
): number | null {
  if (!config) return null;
  const v = config.max_submissions;
  return typeof v === 'number' ? v : null;
}
