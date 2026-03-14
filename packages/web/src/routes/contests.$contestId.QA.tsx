import { useTranslation } from '@broccoli/web-sdk/i18n';
import {
  Badge,
  Button,
  Card,
  CardContent,
  Skeleton,
} from '@broccoli/web-sdk/ui';
import {
  Filter,
  LogIn,
  Mail,
  Megaphone,
  MessageCircle,
} from 'lucide-react';
import { useState } from 'react';
import { Link, useParams } from 'react-router';

import { useAuth } from '@/features/auth/hooks/use-auth';
import type { ClarificationType } from '@/features/clarification/api/types';
import { AskQuestionDialog } from '@/features/clarification/components/AskQuestionDialog';
import { ClarificationCard } from '@/features/clarification/components/ClarificationCard';
import { PostAnnouncementDialog } from '@/features/clarification/components/PostAnnouncementDialog';
import { SendDirectMessageDialog } from '@/features/clarification/components/SendDirectMessageDialog';
import {
  useClarifications,
  useCreateClarification,
  useReplyClarification,
} from '@/features/clarification/hooks/use-clarifications';

type FilterTab = 'all' | ClarificationType;

export default function ContestQAPage() {
  const { t } = useTranslation();
  const { user } = useAuth();
  const { contestId } = useParams();
  const id = Number(contestId ?? '0');
  const [activeTab, setActiveTab] = useState<FilterTab>('all');

  const isAdmin = !!user?.permissions?.includes('contest:manage');

  const { data: clarifications = [], isLoading } = useClarifications(
    id,
    !!user && !!contestId && !Number.isNaN(id),
  );
  const createMutation = useCreateClarification(id);
  const replyMutation = useReplyClarification(id);

  if (!contestId || Number.isNaN(Number(contestId))) {
    return <div className="text-2xl font-bold">{t('contests.notFound')}</div>;
  }

  const filtered =
    activeTab === 'all'
      ? clarifications
      : clarifications.filter((c) => c.clarification_type === activeTab);

  const showDmTab =
    isAdmin ||
    clarifications.some((c) => c.clarification_type === 'direct_message');

  const tabs: { key: FilterTab; label: string; icon: typeof MessageCircle }[] =
    [
      { key: 'all', label: 'All', icon: Filter },
      { key: 'announcement', label: 'Announcements', icon: Megaphone },
      { key: 'question', label: 'Q & A', icon: MessageCircle },
      ...(showDmTab
        ? [
            {
              key: 'direct_message' as FilterTab,
              label: isAdmin ? 'Direct Messages' : 'Messages',
              icon: Mail,
            },
          ]
        : []),
    ];

  return (
    <div className="flex flex-col gap-6 p-6 max-w-5xl mx-auto">
      {/* Header */}
      <div className="flex flex-col md:flex-row md:items-center justify-between gap-4">
        <div className="flex items-center gap-3">
          <MessageCircle className="h-6 w-6 text-primary" />
          <h1 className="text-2xl font-bold">{t('sidebar.qa')}</h1>
          <Badge variant="outline" className="ml-2">
            {filtered.length}
          </Badge>
        </div>

        <div className="flex items-center gap-2">
          {user ? (
            <>
              {isAdmin && (
                <>
                  <PostAnnouncementDialog
                    onSubmit={(content) =>
                      createMutation.mutate({
                        content,
                        clarification_type: 'announcement',
                      })
                    }
                  />
                  <SendDirectMessageDialog
                    contestId={id}
                    onSubmit={(content, recipientId) =>
                      createMutation.mutate({
                        content,
                        clarification_type: 'direct_message',
                        recipient_id: recipientId,
                      })
                    }
                  />
                </>
              )}
              <AskQuestionDialog
                onSubmit={(content) =>
                  createMutation.mutate({
                    content,
                    clarification_type: 'question',
                  })
                }
              />
            </>
          ) : (
            <Button asChild variant="outline">
              <Link to="/login">
                <LogIn className="h-4 w-4 mr-2" />
                {t('auth.loginToAsk')}
              </Link>
            </Button>
          )}
        </div>
      </div>

      {/* Tabs */}
      <div className="flex items-center gap-2 bg-muted p-1 rounded-md w-fit">
        {tabs.map(({ key, label, icon: Icon }) => (
          <Button
            key={key}
            size="sm"
            variant={activeTab === key ? 'default' : 'ghost'}
            onClick={() => setActiveTab(key)}
            className="h-8 gap-1.5"
          >
            <Icon className="h-3.5 w-3.5" />
            {label}
          </Button>
        ))}
      </div>

      {/* Content */}
      <div className="space-y-4">
        {isLoading ? (
          <div className="space-y-4">
            <Skeleton className="h-32 w-full" />
            <Skeleton className="h-32 w-full" />
          </div>
        ) : filtered.length === 0 ? (
          <Card className="border-dashed">
            <CardContent className="flex flex-col items-center justify-center py-10 text-muted-foreground">
              <MessageCircle className="h-10 w-10 mb-2 opacity-20" />
              <p>{t('contest.qa.empty')}</p>
            </CardContent>
          </Card>
        ) : (
          filtered.map((c) => (
            <ClarificationCard
              key={c.id}
              clarification={c}
              isAdmin={isAdmin}
              currentUserId={user?.id ?? -1}
              onReply={(content, isPublic) =>
                replyMutation.mutate({
                  clarificationId: c.id,
                  content,
                  is_public: isPublic,
                })
              }
            />
          ))
        )}
      </div>
    </div>
  );
}
