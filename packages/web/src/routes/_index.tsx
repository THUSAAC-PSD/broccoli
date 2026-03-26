import { useApiClient } from '@broccoli/web-sdk/api';
import { useAuth } from '@broccoli/web-sdk/auth';
import { useTranslation } from '@broccoli/web-sdk/i18n';
import { useQuery } from '@tanstack/react-query';
import { Trophy } from 'lucide-react';
import { useEffect } from 'react';
import { useNavigate } from 'react-router';

import { ListSkeleton } from '@/components/ListSkeleton';
import { GuestWelcome } from '@/features/auth/components/GuestWelcome';
import { ContestSelector } from '@/features/contest/components/ContestSelector';
import { useContest } from '@/features/contest/contexts/contest-context';

export default function Index() {
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
      return data.data;
    },
  });

  // Auto-select contest if there's exactly one
  useEffect(() => {
    if (!user) {
      return;
    }
    if (user && user.role === 'admin') {
      navigate('/admin');
      return;
    }
    if (contests && contests.length === 1 && !contestId) {
      setContest(contests[0].id, contests[0].title);
      navigate(`/contests/${contests[0].id}`);
    }
  }, [contests, contestId, setContest, navigate, user]);

  // Admin user, redirect to admin dashboard
  if (user && user.role === 'admin') {
    return <></>;
  }

  // Avoid rendering homepage content when we're about to auto-redirect
  if (contests && contests.length === 1 && user && !contestId) {
    return null;
  }

  // Not logged in
  if (!user) {
    return <GuestWelcome />;
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
      <div className="flex flex-col items-center justify-center min-h-[60vh] text-center px-4">
        <div className="flex h-16 w-16 items-center justify-center rounded-2xl bg-muted mb-6">
          <Trophy className="h-8 w-8 text-muted-foreground" />
        </div>
        <h1 className="text-2xl font-bold tracking-tight mb-2">
          {t('homepage.title')}
        </h1>
        <p className="text-muted-foreground max-w-md">
          {t('homepage.noContests')}
        </p>
      </div>
    );
  }

  // Multiple contests
  if (contests.length > 1 && !contestId) {
    return (
      <div className="flex flex-col gap-6 p-6 max-w-2xl mx-auto">
        <ContestSelector contests={contests} />
      </div>
    );
  }
}
