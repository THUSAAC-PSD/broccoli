import { useTranslation } from '@broccoli/web-sdk/i18n';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import {
  CheckCircle2,
  Clock,
  Lock,
  Megaphone,
  MessageCircle,
  Send,
  User,
} from 'lucide-react';
import { useState } from 'react';
import { useParams } from 'react-router';

import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardHeader } from '@/components/ui/card';
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from '@/components/ui/dialog';
import { Skeleton } from '@/components/ui/skeleton';
import { Textarea } from '@/components/ui/textarea';
import { useAuth } from '@/features/auth/hooks/use-auth';
import { formatTime } from '@/lib/utils';

// --- Types ---

type QAStatus = 'pending' | 'answered';
type AnswerType = 'private' | 'public';

interface Question {
  id: number;
  contestId: number;
  askerId: number;
  askerName: string;
  content: string;
  createTime: string;
  status: QAStatus;
  answer?: {
    content: string;
    responderId: number;
    responderName: string;
    answerTime: string;
    type: AnswerType;
  };
}

// --- Mock Data ---

const INITIAL_QUESTIONS: Question[] = [
  {
    id: 1,
    contestId: 1,
    askerId: 101,
    askerName: 'Me (Participant)',
    content:
      '请问第一题的输入范围是不是写错了？题目里说 n < 100，但样例是 1000。',
    createTime: new Date(Date.now() - 1000 * 60 * 30).toISOString(),
    status: 'answered',
    answer: {
      content: '抱歉，题目描述有误，已修正为 n < 2000。此消息为全员公告。',
      responderName: 'Admin',
      responderId: 999,
      answerTime: new Date(Date.now() - 1000 * 60 * 25).toISOString(),
      type: 'public',
    },
  },
  {
    id: 2,
    contestId: 1,
    askerId: 102,
    askerName: 'AnotherUser',
    content: 'C 题提交一直编译错误，但我本地是好的。',
    createTime: new Date(Date.now() - 1000 * 60 * 15).toISOString(),
    status: 'answered',
    answer: {
      content: '请检查是否使用了特定编译器版本的特性。',
      responderName: 'Judge',
      responderId: 999,
      answerTime: new Date(Date.now() - 1000 * 60 * 10).toISOString(),
      type: 'private',
    },
  },
  {
    id: 3,
    contestId: 1,
    askerId: 101,
    askerName: 'Me (Participant)',
    content: '我的代码为什么在这个测试点过不去？',
    createTime: new Date(Date.now() - 1000 * 60 * 5).toISOString(),
    status: 'pending',
  },
  {
    id: 4,
    contestId: 1,
    askerId: 103,
    askerName: 'Newbie',
    content: '可以带纸质字典进场吗？',
    createTime: new Date(Date.now() - 1000 * 60 * 2).toISOString(),
    status: 'pending',
  },
];

// --- Components ---

export default function ContestQAPage() {
  const { t } = useTranslation();
  const { user } = useAuth();
  const { contestId } = useParams();
  const id = Number(contestId);
  const queryClient = useQueryClient();
  const isAdminMode = true; // For demo purposes, you can toggle this to false to see participant view

  const userId = user?.id || -1;
  const userName = user?.username || 'Unknown User';

  const { data: questions = [], isLoading } = useQuery({
    queryKey: ['contest-qa', id],
    queryFn: async () => {
      return [...INITIAL_QUESTIONS];
    },
  });

  const askMutation = useMutation({
    mutationFn: async (content: string) => {
      const newQ: Question = {
        id: Math.random(),
        contestId: id,
        askerId: userId,
        askerName: userName,
        content,
        createTime: new Date().toISOString(),
        status: 'pending',
      };
      INITIAL_QUESTIONS.unshift(newQ);
      return newQ;
    },
    onMutate: async (newContent) => {
      await queryClient.cancelQueries({ queryKey: ['contest-qa', id] });

      const previousQuestions = queryClient.getQueryData(['contest-qa', id]);

      queryClient.setQueryData(
        ['contest-qa', id],
        (old: Question[] | undefined) => {
          const optimisticQuestion: Question = {
            id: Date.now(),
            contestId: id,
            askerId: userId,
            askerName: userName,
            content: newContent,
            createTime: new Date().toISOString(),
            status: 'pending',
          };
          return [optimisticQuestion, ...(old || [])];
        },
      );

      return { previousQuestions };
    },

    onError: (...args) => {
      const context = args[2];
      queryClient.setQueryData(['contest-qa', id], context?.previousQuestions);
    },

    onSettled: () => {
      queryClient.invalidateQueries({ queryKey: ['contest-qa', id] });
    },
  });

  const answerMutation = useMutation({
    mutationFn: async (payload: {
      questionId: number;
      content: string;
      type: AnswerType;
    }) => {
      const qIndex = INITIAL_QUESTIONS.findIndex(
        (q) => q.id === payload.questionId,
      );
      if (qIndex > -1) {
        INITIAL_QUESTIONS[qIndex].status = 'answered';
        INITIAL_QUESTIONS[qIndex].answer = {
          content: payload.content,
          responderName: 'You (Admin)',
          responderId: 999,
          answerTime: new Date().toISOString(),
          type: payload.type,
        };
      }
    },
    onMutate: async (payload) => {
      await queryClient.cancelQueries({ queryKey: ['contest-qa', id] });
      const previousQuestions = queryClient.getQueryData(['contest-qa', id]);

      queryClient.setQueryData(
        ['contest-qa', id],
        (old: Question[] | undefined) => {
          if (!old) return [];
          return old.map((q) => {
            if (q.id === payload.questionId) {
              return {
                ...q,
                status: 'answered' as const,
                answer: {
                  content: payload.content,
                  responderName: 'You (Admin)',
                  answerTime: new Date().toISOString(),
                  type: payload.type,
                },
              };
            }
            return q;
          });
        },
      );

      return { previousQuestions };
    },
    onError: (...args) => {
      const context = args[2];
      queryClient.setQueryData(['contest-qa', id], context?.previousQuestions);
    },
    onSettled: () => {
      queryClient.invalidateQueries({ queryKey: ['contest-qa', id] });
    },
  });

  // --- Logic ---

  const visibleQuestions = questions.filter((q) => {
    if (isAdminMode) return true;

    const isMyQuestion = q.askerId === userId;
    const isPublicAnswer =
      q.status === 'answered' && q.answer?.type === 'public';

    if (q.status === 'pending') return isMyQuestion;

    return isMyQuestion || isPublicAnswer;
  });

  return (
    <div className="flex flex-col gap-6 p-6 max-w-5xl mx-auto">
      <div className="flex flex-col md:flex-row md:items-center justify-between gap-4">
        <div className="flex items-center gap-3">
          <MessageCircle className="h-6 w-6 text-primary" />
          <h1 className="text-2xl font-bold">{'Q & A'}</h1>
          <Badge variant="outline" className="ml-2">
            {visibleQuestions.length} {'Threads'}
          </Badge>
        </div>

        <div className="flex items-center gap-4">
          <AskQuestionDialog
            onSubmit={(content) => askMutation.mutate(content)}
          />
        </div>
      </div>

      <div className="space-y-4">
        {isLoading ? (
          <div className="space-y-4">
            <Skeleton className="h-32 w-full" />
            <Skeleton className="h-32 w-full" />
          </div>
        ) : visibleQuestions.length === 0 ? (
          <Card className="border-dashed">
            <CardContent className="flex flex-col items-center justify-center py-10 text-muted-foreground">
              <MessageCircle className="h-10 w-10 mb-2 opacity-20" />
              <p>{t('contest.qa.empty') || 'No questions yet.'}</p>
            </CardContent>
          </Card>
        ) : (
          visibleQuestions.map((q) => (
            <QuestionCard
              key={q.id}
              question={q}
              isAdmin={isAdminMode}
              onAnswer={(content, type) =>
                answerMutation.mutate({
                  questionId: q.id,
                  content,
                  type,
                })
              }
            />
          ))
        )}
      </div>
    </div>
  );
}

function QuestionCard({
  question,
  isAdmin,
  onAnswer,
}: {
  question: Question;
  isAdmin: boolean;
  onAnswer: (content: string, type: AnswerType) => void;
}) {
  const [answerContent, setAnswerContent] = useState('');
  const [answerType, setAnswerType] = useState<AnswerType>('private');
  const { locale } = useTranslation();
  const isPending = question.status === 'pending';

  const handleSubmit = () => {
    if (!answerContent.trim()) return;
    onAnswer(answerContent, answerType);
    setAnswerContent('');
  };

  return (
    <Card
      className={`overflow-hidden transition-colors ${isPending ? 'border-l-4 border-l-yellow-400/50' : 'border-l-4 border-l-green-500/50'}`}
    >
      <CardHeader className="pb-3 bg-muted/20">
        <div className="flex justify-between items-start">
          <div className="flex flex-col gap-1">
            <div className="flex items-center gap-2 text-sm font-medium">
              <User className="h-4 w-4 text-muted-foreground" />
              <span>{question.askerName}</span>
              <span className="text-muted-foreground text-xs font-normal flex items-center gap-1">
                <Clock className="h-3 w-3" />{' '}
                {formatTime(question.createTime, locale)}
              </span>
            </div>
          </div>
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
        </div>
      </CardHeader>

      <CardContent className="pt-4 space-y-4">
        <div className="text-base font-medium whitespace-pre-wrap">
          {question.content}
        </div>

        {!isPending && question.answer && (
          <div className="bg-muted/50 rounded-lg p-4 border relative mt-4">
            <div className="absolute top-3 right-3">
              {question.answer.type === 'public' ? (
                <Badge
                  variant="outline"
                  className="gap-1 border-blue-200 bg-blue-50 text-blue-700"
                >
                  <Megaphone className="h-3 w-3" /> Announcement
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
              {question.answer.responderName}
              <span className="text-xs text-muted-foreground font-normal">
                at {formatTime(question.answer.answerTime, locale)}
              </span>
            </div>
            <div className="text-sm text-foreground/90 whitespace-pre-wrap pl-6">
              {question.answer.content}
            </div>
          </div>
        )}

        {isAdmin && isPending && (
          <div className="mt-4 pt-4 border-t space-y-4 animate-in fade-in zoom-in-95 duration-200">
            <div className="space-y-2">
              <div className="text-sm font-medium">Reply to this question</div>
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
                  variant={answerType === 'private' ? 'default' : 'ghost'}
                  onClick={() => setAnswerType('private')}
                  className="h-8 gap-2"
                >
                  <Lock className="h-3 w-3" /> Private
                </Button>
                <Button
                  size="sm"
                  variant={answerType === 'public' ? 'default' : 'ghost'}
                  onClick={() => setAnswerType('public')}
                  className="h-8 gap-2"
                >
                  <Megaphone className="h-3 w-3" /> Public
                </Button>
              </div>

              <Button
                onClick={handleSubmit}
                disabled={!answerContent.trim()}
                size="sm"
              >
                <Send className="h-4 w-4 mr-2" />
                Submit Answer
              </Button>
            </div>
          </div>
        )}
      </CardContent>
    </Card>
  );
}

function AskQuestionDialog({
  onSubmit,
}: {
  onSubmit: (content: string) => void;
}) {
  const [open, setOpen] = useState(false);
  const [content, setContent] = useState('');

  const handleSubmit = () => {
    onSubmit(content);
    setContent('');
    setOpen(false);
  };

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger asChild>
        <Button>
          <MessageCircle className="h-4 w-4 mr-2" />
          Ask Question
        </Button>
      </DialogTrigger>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Ask a Question</DialogTitle>
        </DialogHeader>
        <div className="py-4">
          <Textarea
            placeholder="Describe your issue clearly (e.g., Problem ID, specific error)..."
            className="min-h-[150px]"
            value={content}
            onChange={(e) => setContent(e.target.value)}
          />
          <p className="text-xs text-muted-foreground mt-2">
            Your question will be visible only to judges until they decide to
            make it public.
          </p>
        </div>
        <DialogFooter>
          <Button variant="outline" onClick={() => setOpen(false)}>
            Cancel
          </Button>
          <Button onClick={handleSubmit} disabled={!content.trim()}>
            Send
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
