import { useTranslation } from '@broccoli/web-sdk/i18n';
import { Trophy } from 'lucide-react';
import { useParams } from 'react-router';

import { PageLayout } from '@/components/PageLayout';
import { useAuth } from '@/features/auth/hooks/use-auth';
import { ContestAdminActions } from '@/features/contest/components/ContestAdminActions';
import { ContestCountdown } from '@/features/contest/components/ContestCountdown';
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
  const enrollState = useContestEnroll({
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
      contentClassName="flex flex-col gap-4"
    >
      <div className="flex flex-col gap-4 lg:flex-row">
        <div className="flex-1 min-w-0 space-y-4">
          <ContestProblemsCard contestId={id} />
        </div>
        <div className="flex flex-col w-full lg:w-80 lg:shrink-0 lg:sticky lg:top-6 h-fit gap-4">
          <ContestCountdown />
          {enrollState.canShowEnrollCard && (
            <ContestEnrollCard
              onEnroll={enrollState.enroll}
              isPending={enrollState.isPending}
            />
          )}
          {enrollState.canShowUnregisterButton && (
            <ContestEnrollCard
              onEnroll={enrollState.enroll}
              isPending={enrollState.isPending}
              onUnregister={enrollState.unregister}
              isUnregistering={enrollState.isUnregistering}
              showUnregister
            />
          )}
          <ContestAdminActions />
        </div>
      </div>
    </PageLayout>
  );
}
