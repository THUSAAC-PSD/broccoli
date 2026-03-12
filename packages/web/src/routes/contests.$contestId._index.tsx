import { useTranslation } from '@broccoli/web-sdk/i18n';
import { Trophy } from 'lucide-react';
import { useParams } from 'react-router';

import { PageLayout } from '@/components/PageLayout';
import { useAuth } from '@/features/auth/hooks/use-auth';
import { ContestEnrollCard } from '@/features/contest/components/ContestEnrollCard';
import { ContestProblemsCard } from '@/features/contest/components/ContestProblemsCard';
import { useContestEnroll } from '@/features/contest/hooks/use-contest-enroll';
import { useContestInfo } from '@/features/contest/hooks/use-contest-info';

export default function ContestOverviewPage() {
  const { t } = useTranslation();
  const { user } = useAuth();
  const { contestId } = useParams();
  const id = Number(contestId);
  const { contest } = useContestInfo(id);
  const canManageContest = !!user?.permissions.includes('contest:manage');
  const { canShowEnrollCard, enroll, isPending } = useContestEnroll({
    contestId: id,
    contest,
    canManageContest,
  });

  if (!contestId || Number.isNaN(id)) {
    return (
      <div className="flex flex-col gap-4 p-6">
        <h1 className="text-2xl font-bold">{t('contests.notFound')}</h1>
      </div>
    );
  }

  return (
    <PageLayout
      pageId="contest-overview"
      title={contest?.title ?? t('contests.title')}
      subtitle={contest?.description}
      icon={<Trophy className="h-6 w-6 text-primary" />}
      contentClassName="grid grid-cols-1 lg:grid-cols-[1fr_320px] gap-4 items-start"
    >
      {canShowEnrollCard ? (
        <ContestEnrollCard onEnroll={enroll} isPending={isPending} />
      ) : null}
      <ContestProblemsCard contestId={id} />
    </PageLayout>
  );
}
