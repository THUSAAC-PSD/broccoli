import { useTranslation } from '@broccoli/web-sdk/i18n';
import {
  Badge,
  Button,
  Card,
  CardContent,
  CardHeader,
  Textarea,
} from '@broccoli/web-sdk/ui';
import { formatTime } from '@broccoli/web-sdk/utils';
import {
  CheckCircle2,
  Clock,
  Lock,
  Mail,
  Megaphone,
  Send,
  User,
} from 'lucide-react';
import { useState } from 'react';

import type { Clarification } from '../api/types';

interface ClarificationCardProps {
  clarification: Clarification;
  isAdmin: boolean;
  currentUserId: number;
  onReply: (content: string, isPublic: boolean) => void;
}

export function ClarificationCard({
  clarification,
  isAdmin,
  currentUserId,
  onReply,
}: ClarificationCardProps) {
  const [answerContent, setAnswerContent] = useState('');
  const [replyIsPublic, setReplyIsPublic] = useState(false);
  const { locale } = useTranslation();

  const isAnnouncement = clarification.clarification_type === 'announcement';
  const isDirectMessage =
    clarification.clarification_type === 'direct_message';
  const isPending = !clarification.reply_content && !isAnnouncement;
  const isOwn = clarification.author_id === currentUserId;

  const handleSubmit = () => {
    if (!answerContent.trim()) return;
    onReply(answerContent, replyIsPublic);
    setAnswerContent('');
  };

  const borderClass = isAnnouncement
    ? 'border-l-4 border-l-blue-500/50'
    : isPending
      ? 'border-l-4 border-l-yellow-400/50'
      : 'border-l-4 border-l-green-500/50';

  return (
    <Card className={`overflow-hidden transition-colors ${borderClass}`}>
      <CardHeader className="pb-3 bg-muted/20">
        <div className="flex justify-between items-start">
          <div className="flex flex-col gap-1">
            <div className="flex items-center gap-2 text-sm font-medium">
              {isAnnouncement ? (
                <Megaphone className="h-4 w-4 text-blue-600" />
              ) : isDirectMessage ? (
                <Mail className="h-4 w-4 text-purple-600" />
              ) : (
                <User className="h-4 w-4 text-muted-foreground" />
              )}
              <span>
                {clarification.author_name}
                {isOwn && (
                  <span className="text-xs text-muted-foreground ml-1">
                    (you)
                  </span>
                )}
              </span>
              {isDirectMessage && clarification.recipient_name && (
                <span className="text-xs text-muted-foreground">
                  → {clarification.recipient_name}
                </span>
              )}
              <span className="text-muted-foreground text-xs font-normal flex items-center gap-1">
                <Clock className="h-3 w-3" />{' '}
                {formatTime(clarification.created_at, locale)}
              </span>
            </div>
          </div>
          <TypeBadge
            clarification={clarification}
            isPending={isPending}
            isAnnouncement={isAnnouncement}
            isDirectMessage={isDirectMessage}
          />
        </div>
      </CardHeader>

      <CardContent className="pt-4 space-y-4">
        <div className="text-base font-medium whitespace-pre-wrap">
          {clarification.content}
        </div>

        {clarification.reply_content && (
          <ReplyDisplay clarification={clarification} />
        )}

        {isAdmin && isPending && !isAnnouncement && (
          <div className="mt-4 pt-4 border-t space-y-4 animate-in fade-in zoom-in-95 duration-200">
            <div className="space-y-2">
              <div className="text-sm font-medium">Reply</div>
              <Textarea
                placeholder="Type your answer here..."
                value={answerContent}
                onChange={(e) => setAnswerContent(e.target.value)}
                className="min-h-[100px]"
              />
            </div>

            <div className="flex flex-col sm:flex-row justify-between items-center gap-4">
              <div className="flex items-center gap-2 bg-muted p-1 rounded-md">
                <Button
                  size="sm"
                  variant={!replyIsPublic ? 'default' : 'ghost'}
                  onClick={() => setReplyIsPublic(false)}
                  className="h-8 gap-2"
                >
                  <Lock className="h-3 w-3" /> Private
                </Button>
                <Button
                  size="sm"
                  variant={replyIsPublic ? 'default' : 'ghost'}
                  onClick={() => setReplyIsPublic(true)}
                  className="h-8 gap-2"
                >
                  <Megaphone className="h-3 w-3" /> Reply to all
                </Button>
              </div>

              <Button
                onClick={handleSubmit}
                disabled={!answerContent.trim()}
                size="sm"
              >
                <Send className="h-4 w-4 mr-2" />
                Submit
              </Button>
            </div>
          </div>
        )}
      </CardContent>
    </Card>
  );
}

function TypeBadge({
  clarification: _,
  isPending,
  isAnnouncement,
  isDirectMessage,
}: {
  clarification: Clarification;
  isPending: boolean;
  isAnnouncement: boolean;
  isDirectMessage: boolean;
}) {
  if (isAnnouncement) {
    return (
      <Badge
        variant="outline"
        className="gap-1 border-blue-200 bg-blue-50 text-blue-700"
      >
        <Megaphone className="h-3 w-3" /> Announcement
      </Badge>
    );
  }
  if (isDirectMessage) {
    return (
      <Badge
        variant="outline"
        className="gap-1 border-purple-200 bg-purple-50 text-purple-700"
      >
        <Mail className="h-3 w-3" /> Direct
      </Badge>
    );
  }
  return (
    <Badge
      variant={isPending ? 'secondary' : 'default'}
      className={
        isPending
          ? 'text-yellow-600 bg-yellow-50 hover:bg-yellow-100'
          : 'bg-green-600 hover:bg-green-700'
      }
    >
      {isPending ? 'Pending' : 'Answered'}
    </Badge>
  );
}

function ReplyDisplay({ clarification }: { clarification: Clarification }) {
  const { locale } = useTranslation();

  return (
    <div className="bg-muted/50 rounded-lg p-4 border relative mt-4">
      <div className="absolute top-3 right-3">
        {clarification.reply_is_public ? (
          <Badge
            variant="outline"
            className="gap-1 border-blue-200 bg-blue-50 text-blue-700"
          >
            <Megaphone className="h-3 w-3" /> Public
          </Badge>
        ) : (
          <Badge
            variant="outline"
            className="gap-1 border-amber-200 bg-amber-50 text-amber-700"
          >
            <Lock className="h-3 w-3" /> Private
          </Badge>
        )}
      </div>

      <div className="text-sm font-semibold mb-1 flex items-center gap-2">
        <CheckCircle2 className="h-4 w-4 text-green-600" />
        {clarification.reply_author_name ?? 'Admin'}
        {clarification.replied_at && (
          <span className="text-xs text-muted-foreground font-normal">
            at {formatTime(clarification.replied_at, locale)}
          </span>
        )}
      </div>
      <div className="text-sm text-foreground/90 whitespace-pre-wrap pl-6">
        {clarification.reply_content}
      </div>
    </div>
  );
}
