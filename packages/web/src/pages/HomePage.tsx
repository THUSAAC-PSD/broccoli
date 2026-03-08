import type { ContestListItem } from '@broccoli/web-sdk';
import { useApiClient } from '@broccoli/web-sdk/api';
import { useTranslation } from '@broccoli/web-sdk/i18n';
import { useQuery } from '@tanstack/react-query';
import { useEffect } from 'react';
import { Link, useNavigate } from 'react-router';

import { Button } from '@/components/ui/button';
import {
  Card,
  CardContent,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
} from '@/components/ui/card';
import { Skeleton } from '@/components/ui/skeleton';
import { useAuth } from '@/contexts/auth-context';
import { useContest } from '@/contexts/contest-context';

function ListSkeleton() {
  return (
    <div className="space-y-3">
      <Skeleton className="h-8 w-full" />
      <Skeleton className="h-8 w-full" />
      <Skeleton className="h-8 w-full" />
    </div>
  );
}

function ContestSelector({
  contests,
  onSelect,
}: {
  contests: ContestListItem[];
  onSelect: (contest: ContestListItem) => void;
}) {
  const { t, locale } = useTranslation();

  return (
    <Card>
      <CardHeader>
        <CardTitle>{t('homepage.selectContest')}</CardTitle>
        <CardDescription>{t('homepage.selectContestDesc')}</CardDescription>
      </CardHeader>
      <CardContent>
        <div className="space-y-2">
          {contests.map((contest) => (
            <button
              key={contest.id}
              onClick={() => onSelect(contest)}
              className="flex w-full items-center justify-between rounded-lg border p-4 text-left transition-colors hover:bg-accent"
            >
              <div>
                <div className="font-medium">{contest.title}</div>
                <div className="text-sm text-muted-foreground">
                  {new Date(contest.start_time).toLocaleDateString(locale)} -{' '}
                  {new Date(contest.end_time).toLocaleDateString(locale)}
                </div>
              </div>
            </button>
          ))}
        </div>
      </CardContent>
    </Card>
  );
}

export function HomePage() {
  const { t } = useTranslation();
  const { user } = useAuth();
  const apiClient = useApiClient();
  const { contestId, setContest } = useContest();
  const navigate = useNavigate();

  const { data: contests, isLoading: isContestsLoading } = useQuery({
    queryKey: ['dashboard-contests'],
    enabled: !!user,
    queryFn: async () => {
      const { data, error } = await apiClient.GET('/contests', {
        params: {
          query: {
            page: 1,
            per_page: 100,
            sort_by: 'start_time',
            sort_order: 'desc',
          },
        },
      });
      if (error) throw error;
      return data.data as ContestListItem[];
    },
  });

  // Auto-select contest if there's exactly one
  useEffect(() => {
    if (!user) {
      return;
    }
    if (user && user.role === 'admin') {
      //No api to judge if the user has access to overview page, waiting
      navigate('/overview');
      return;
    }
    if (contests && contests.length === 1 && !contestId) {
      setContest(contests[0].id, contests[0].title);
      navigate(`/contests/${contests[0].id}`);
    }
  }, [contests, contestId, setContest]);

  // Admin user, redirect to admin dashboard
  if (user && user.role === 'admin') {
    return <></>;
  }

  // Not logged in
  if (!user) {
    return (
      <div className="flex flex-col gap-6 p-6">
        <Card>
          <CardHeader>
            <CardTitle>welcome</CardTitle>
            <CardDescription>
              Welcome to use Broccoli! Please sign in to view your contests and
              start competing.
            </CardDescription>
          </CardHeader>
          <CardFooter className="gap-2">
            <Button asChild>
              <Link to="/login">{t('nav.signIn')}</Link>
            </Button>
            <Button variant="outline" asChild>
              <Link to="/register">{t('nav.signUp')}</Link>
            </Button>
          </CardFooter>
        </Card>
      </div>
    );
  }

  // Loading contests
  if (isContestsLoading) {
    return (
      <div className="flex flex-col gap-6 p-6">
        <h1 className="text-2xl font-bold">{t('homepage.title')}</h1>
        <ListSkeleton />
      </div>
    );
  }

  if (contests && contests.length === 1) {
    return <></>;
  }

  // No contests
  if (!contests?.length) {
    return (
      <div className="flex flex-col gap-6 p-6">
        <h1 className="text-2xl font-bold">Broccoli Online Judge</h1>
        <Card>
          <CardContent className="pt-6">
            <p className="text-sm text-muted-foreground">
              There's no contest available for you at the moment. Please check
              back later or contact the administrator if you think this is a
              mistake.
            </p>
          </CardContent>
        </Card>
      </div>
    );
  }

  // Multiple contests, none selected yet
  if (contests.length > 1 && !contestId) {
    return (
      <div className="flex flex-col gap-6 p-6">
        <ContestSelector
          contests={contests}
          onSelect={(c) => navigate(`/contests/${c.id}`)}
        />
      </div>
    );
  }
}
