import type { SubmissionStatus, Verdict } from '@broccoli/sdk';
import type { ApiClient } from '@broccoli/sdk/api';
import { useTranslation } from '@broccoli/sdk/i18n';
import { Slot } from '@broccoli/sdk/react';
import { Trophy } from 'lucide-react';

import type { DataTableColumn } from '@/components/ui/data-table';
import { DataTable } from '@/components/ui/data-table';
import type { ServerTableParams } from '@/hooks/use-server-table';

// --- Types aligned with API schemas ---

/** Per-problem result for a participant, computed from submissions */
interface ProblemResult {
  verdict: Verdict | null;
  status: SubmissionStatus;
  attempts: number;
  /** Time in minutes from contest start, if accepted */
  time?: number;
  /** Score for this problem */
  score: number | null;
}

/** A row in the standings table, computed client-side from submissions */
interface StandingRow {
  rank: number;
  user_id: number;
  username: string;
  solved: number;
  penalty: number;
  total_score: number;
  problems: Record<string, ProblemResult>;
}

// Problem labels from ContestProblemResponse
const PROBLEM_LABELS = ['A', 'B', 'C', 'D', 'E'];

// --- Mock standings (simulates client-side computation from submissions) ---

const ALL_STANDINGS: StandingRow[] = [
  {
    rank: 1,
    user_id: 1,
    username: 'alice',
    solved: 5,
    penalty: 312,
    total_score: 500,
    problems: {
      A: {
        verdict: 'Accepted',
        status: 'Judged',
        attempts: 1,
        time: 8,
        score: 100,
      },
      B: {
        verdict: 'Accepted',
        status: 'Judged',
        attempts: 1,
        time: 25,
        score: 100,
      },
      C: {
        verdict: 'Accepted',
        status: 'Judged',
        attempts: 2,
        time: 67,
        score: 100,
      },
      D: {
        verdict: 'Accepted',
        status: 'Judged',
        attempts: 1,
        time: 110,
        score: 100,
      },
      E: {
        verdict: 'Accepted',
        status: 'Judged',
        attempts: 3,
        time: 180,
        score: 100,
      },
    },
  },
  {
    rank: 2,
    user_id: 2,
    username: 'bob',
    solved: 4,
    penalty: 245,
    total_score: 400,
    problems: {
      A: {
        verdict: 'Accepted',
        status: 'Judged',
        attempts: 1,
        time: 5,
        score: 100,
      },
      B: {
        verdict: 'Accepted',
        status: 'Judged',
        attempts: 2,
        time: 30,
        score: 100,
      },
      C: {
        verdict: 'Accepted',
        status: 'Judged',
        attempts: 1,
        time: 55,
        score: 100,
      },
      D: {
        verdict: 'Accepted',
        status: 'Judged',
        attempts: 4,
        time: 145,
        score: 100,
      },
      E: { verdict: 'WrongAnswer', status: 'Judged', attempts: 3, score: 0 },
    },
  },
  {
    rank: 3,
    user_id: 3,
    username: 'charlie',
    solved: 4,
    penalty: 280,
    total_score: 400,
    problems: {
      A: {
        verdict: 'Accepted',
        status: 'Judged',
        attempts: 1,
        time: 12,
        score: 100,
      },
      B: {
        verdict: 'Accepted',
        status: 'Judged',
        attempts: 1,
        time: 35,
        score: 100,
      },
      C: {
        verdict: 'Accepted',
        status: 'Judged',
        attempts: 3,
        time: 90,
        score: 100,
      },
      D: {
        verdict: 'Accepted',
        status: 'Judged',
        attempts: 2,
        time: 160,
        score: 100,
      },
      E: {
        verdict: 'TimeLimitExceeded',
        status: 'Judged',
        attempts: 1,
        score: 0,
      },
    },
  },
  {
    rank: 4,
    user_id: 4,
    username: 'diana',
    solved: 3,
    penalty: 178,
    total_score: 300,
    problems: {
      A: {
        verdict: 'Accepted',
        status: 'Judged',
        attempts: 1,
        time: 10,
        score: 100,
      },
      B: {
        verdict: 'Accepted',
        status: 'Judged',
        attempts: 1,
        time: 40,
        score: 100,
      },
      C: {
        verdict: 'Accepted',
        status: 'Judged',
        attempts: 2,
        time: 108,
        score: 100,
      },
      D: { verdict: 'RuntimeError', status: 'Judged', attempts: 5, score: 0 },
      E: { verdict: null, status: 'Pending', attempts: 0, score: null },
    },
  },
  {
    rank: 5,
    user_id: 5,
    username: 'eve',
    solved: 3,
    penalty: 210,
    total_score: 300,
    problems: {
      A: {
        verdict: 'Accepted',
        status: 'Judged',
        attempts: 2,
        time: 15,
        score: 100,
      },
      B: {
        verdict: 'Accepted',
        status: 'Judged',
        attempts: 1,
        time: 50,
        score: 100,
      },
      C: {
        verdict: 'Accepted',
        status: 'Judged',
        attempts: 1,
        time: 125,
        score: 100,
      },
      D: { verdict: 'WrongAnswer', status: 'Judged', attempts: 2, score: 0 },
      E: { verdict: null, status: 'Pending', attempts: 0, score: null },
    },
  },
  {
    rank: 6,
    user_id: 6,
    username: 'frank',
    solved: 2,
    penalty: 95,
    total_score: 200,
    problems: {
      A: {
        verdict: 'Accepted',
        status: 'Judged',
        attempts: 1,
        time: 20,
        score: 100,
      },
      B: {
        verdict: 'Accepted',
        status: 'Judged',
        attempts: 3,
        time: 75,
        score: 100,
      },
      C: {
        verdict: 'MemoryLimitExceeded',
        status: 'Judged',
        attempts: 4,
        score: 0,
      },
      D: { verdict: null, status: 'Pending', attempts: 0, score: null },
      E: { verdict: null, status: 'Pending', attempts: 0, score: null },
    },
  },
  {
    rank: 7,
    user_id: 7,
    username: 'grace',
    solved: 2,
    penalty: 130,
    total_score: 200,
    problems: {
      A: {
        verdict: 'Accepted',
        status: 'Judged',
        attempts: 1,
        time: 18,
        score: 100,
      },
      B: {
        verdict: 'Accepted',
        status: 'Judged',
        attempts: 2,
        time: 92,
        score: 100,
      },
      C: { verdict: 'WrongAnswer', status: 'Judged', attempts: 2, score: 0 },
      D: { verdict: null, status: 'Pending', attempts: 0, score: null },
      E: { verdict: null, status: 'Pending', attempts: 0, score: null },
    },
  },
  {
    rank: 8,
    user_id: 8,
    username: 'henry',
    solved: 1,
    penalty: 22,
    total_score: 100,
    problems: {
      A: {
        verdict: 'Accepted',
        status: 'Judged',
        attempts: 1,
        time: 22,
        score: 100,
      },
      B: { verdict: null, status: 'CompilationError', attempts: 3, score: 0 },
      C: { verdict: null, status: 'Pending', attempts: 0, score: null },
      D: { verdict: null, status: 'Pending', attempts: 0, score: null },
      E: { verdict: null, status: 'Pending', attempts: 0, score: null },
    },
  },
  {
    rank: 9,
    user_id: 9,
    username: 'iris',
    solved: 1,
    penalty: 45,
    total_score: 100,
    problems: {
      A: {
        verdict: 'Accepted',
        status: 'Judged',
        attempts: 3,
        time: 45,
        score: 100,
      },
      B: { verdict: 'WrongAnswer', status: 'Judged', attempts: 5, score: 0 },
      C: { verdict: null, status: 'Pending', attempts: 0, score: null },
      D: { verdict: null, status: 'Pending', attempts: 0, score: null },
      E: { verdict: null, status: 'Pending', attempts: 0, score: null },
    },
  },
  {
    rank: 10,
    user_id: 10,
    username: 'jack',
    solved: 0,
    penalty: 0,
    total_score: 0,
    problems: {
      A: { verdict: 'WrongAnswer', status: 'Judged', attempts: 2, score: 0 },
      B: { verdict: null, status: 'Pending', attempts: 0, score: null },
      C: { verdict: null, status: 'Pending', attempts: 0, score: null },
      D: { verdict: null, status: 'Pending', attempts: 0, score: null },
      E: { verdict: null, status: 'Pending', attempts: 0, score: null },
    },
  },
];

// Mock server-side fetch
async function fetchStandings(
  _apiClient: ApiClient,
  params: ServerTableParams,
) {
  await new Promise((r) => setTimeout(r, 300));

  let filtered = [...ALL_STANDINGS];

  if (params.search) {
    const q = params.search.toLowerCase();
    filtered = filtered.filter((s) => s.username.toLowerCase().includes(q));
  }

  if (params.sort_by) {
    const order = params.sort_order === 'desc' ? -1 : 1;
    filtered.sort((a, b) => {
      const key = params.sort_by as keyof StandingRow;
      if (key === 'username')
        return order * a.username.localeCompare(b.username);
      return order * ((a[key] as number) - (b[key] as number));
    });
  }

  const total = filtered.length;
  const start = (params.page - 1) * params.per_page;
  const data = filtered.slice(start, start + params.per_page);

  return {
    data,
    pagination: {
      page: params.page,
      per_page: params.per_page,
      total,
      total_pages: Math.ceil(total / params.per_page),
    },
  };
}

// Chart data
const TOP_USERS = ALL_STANDINGS.slice(0, 5).map((s) => s.username);

const SCORE_OVER_TIME = [
  { time: 0, alice: 0, bob: 0, charlie: 0, diana: 0, eve: 0 },
  { time: 10, alice: 1, bob: 1, charlie: 0, diana: 1, eve: 0 },
  { time: 30, alice: 2, bob: 1, charlie: 1, diana: 1, eve: 0 },
  { time: 45, alice: 2, bob: 2, charlie: 1, diana: 1, eve: 1 },
  { time: 60, alice: 2, bob: 2, charlie: 2, diana: 2, eve: 2 },
  { time: 90, alice: 3, bob: 3, charlie: 2, diana: 2, eve: 2 },
  { time: 120, alice: 4, bob: 3, charlie: 3, diana: 3, eve: 3 },
  { time: 150, alice: 4, bob: 4, charlie: 3, diana: 3, eve: 3 },
  { time: 180, alice: 5, bob: 4, charlie: 4, diana: 3, eve: 3 },
];

const DISTRIBUTION = [
  { solved: '0', count: 1 },
  { solved: '1', count: 2 },
  { solved: '2', count: 2 },
  { solved: '3', count: 2 },
  { solved: '4', count: 2 },
  { solved: '5', count: 1 },
];

// --- Cell renderers ---

const VERDICT_COLORS: Record<Verdict, string> = {
  Accepted:
    'bg-green-100 text-green-700 dark:bg-green-950/40 dark:text-green-400',
  WrongAnswer: 'bg-red-100 text-red-600 dark:bg-red-950/30 dark:text-red-400',
  TimeLimitExceeded:
    'bg-amber-100 text-amber-700 dark:bg-amber-950/30 dark:text-amber-400',
  MemoryLimitExceeded:
    'bg-orange-100 text-orange-700 dark:bg-orange-950/30 dark:text-orange-400',
  RuntimeError:
    'bg-purple-100 text-purple-700 dark:bg-purple-950/30 dark:text-purple-400',
  SystemError:
    'bg-gray-100 text-gray-600 dark:bg-gray-800/50 dark:text-gray-400',
};

function ProblemCellContent({ result }: { result: ProblemResult }) {
  if (result.attempts === 0) {
    return <span className="text-muted-foreground/40">-</span>;
  }

  if (result.verdict === 'Accepted') {
    const isFirstTry = result.attempts === 1;
    return (
      <div
        className={`inline-flex flex-col items-center rounded px-2 py-0.5 text-xs font-medium ${
          isFirstTry
            ? VERDICT_COLORS.Accepted
            : 'bg-green-50 text-green-600 dark:bg-green-950/20 dark:text-green-500'
        }`}
      >
        <span>+{result.attempts > 1 ? result.attempts - 1 : ''}</span>
        <span className="text-[10px] opacity-70">{result.time}m</span>
      </div>
    );
  }

  const colorClass = result.verdict
    ? VERDICT_COLORS[result.verdict]
    : 'bg-muted text-muted-foreground';

  return (
    <span
      className={`inline-block rounded px-2 py-0.5 text-xs font-medium ${colorClass}`}
    >
      -{result.attempts}
    </span>
  );
}

function RankBadge({ rank }: { rank: number }) {
  if (rank > 3) return <span>{rank}</span>;
  const colors =
    rank === 1 ? 'bg-amber-400' : rank === 2 ? 'bg-gray-400' : 'bg-amber-700';
  return (
    <span
      className={`inline-flex h-6 w-6 items-center justify-center rounded-full text-xs font-bold text-white ${colors}`}
    >
      {rank}
    </span>
  );
}

// --- Column definitions ---

function useStandingsColumns(): DataTableColumn<StandingRow>[] {
  const { t } = useTranslation();

  return [
    {
      accessorKey: 'rank',
      header: '#',
      size: 60,
      sortKey: 'rank',
      cell: ({ row }) => <RankBadge rank={row.original.rank} />,
    },
    {
      accessorKey: 'username',
      header: t('ranking.user'),
      sortKey: 'username',
      cell: ({ row }) => (
        <span className="font-medium">{row.original.username}</span>
      ),
    },
    {
      accessorKey: 'solved',
      header: t('ranking.solved'),
      size: 80,
      sortKey: 'solved',
      cell: ({ row }) => (
        <span className="font-bold text-primary text-center block">
          {row.original.solved}
        </span>
      ),
    },
    {
      accessorKey: 'total_score',
      header: t('ranking.score'),
      size: 80,
      sortKey: 'total_score',
      cell: ({ row }) => (
        <span className="font-semibold text-center block">
          {row.original.total_score}
        </span>
      ),
    },
    {
      accessorKey: 'penalty',
      header: t('ranking.penalty'),
      size: 90,
      sortKey: 'penalty',
      cell: ({ row }) => (
        <span className="text-muted-foreground text-center block">
          {row.original.penalty}
        </span>
      ),
    },
    ...PROBLEM_LABELS.map(
      (label): DataTableColumn<StandingRow> => ({
        id: `problem-${label}`,
        header: label,
        size: 70,
        cell: ({ row }) => (
          <div className="text-center">
            <ProblemCellContent result={row.original.problems[label]} />
          </div>
        ),
      }),
    ),
  ];
}

// --- Page ---

export function RankingPage() {
  const { t } = useTranslation();
  const standingsColumns = useStandingsColumns();

  return (
    <div className="flex flex-col gap-6 p-6">
      <div className="flex items-center gap-3">
        <Trophy className="h-6 w-6 text-amber-500" />
        <h1 className="text-2xl font-bold">{t('ranking.title')}</h1>
      </div>

      <Slot name="ranking.header" as="div" />

      <Slot
        name="ranking.charts"
        slotProps={{
          data: SCORE_OVER_TIME,
          teams: TOP_USERS,
          distribution: DISTRIBUTION,
        }}
      />

      <DataTable
        columns={standingsColumns}
        queryKey={['standings']}
        fetchFn={fetchStandings}
        searchable
        searchPlaceholder={t('ranking.searchPlaceholder')}
        defaultPerPage={10}
        defaultSortBy="rank"
        defaultSortOrder="asc"
        emptyMessage={t('ranking.empty')}
      />
    </div>
  );
}
