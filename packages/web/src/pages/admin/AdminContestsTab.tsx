import type {
  ContestListItem,
  ContestProblemItem,
  ProblemListItem,
} from '@broccoli/sdk';
import { type ApiClient, useApiClient } from '@broccoli/sdk/api';
import { useTranslation } from '@broccoli/sdk/i18n';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import {
  Check,
  Eye,
  EyeOff,
  List,
  MoreHorizontal,
  Pencil,
  Plus,
  Search,
  Settings,
  Trash2,
  Upload,
  UserPlus,
} from 'lucide-react';
import { useEffect, useState } from 'react';
import { Link } from 'react-router';

import { ResourceConfigDialog, useHasConfigSchemas } from '@/components/config';
import { Markdown } from '@/components/Markdown';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import type { DataTableColumn } from '@/components/ui/data-table';
import { DataTable } from '@/components/ui/data-table';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Separator } from '@/components/ui/separator';
import { Textarea } from '@/components/ui/textarea';
import { useRegistries } from '@/hooks/use-registries';
import type { ServerTableParams } from '@/hooks/use-server-table';

import {
  formatDateTime,
  getContestStatus,
  SwitchField,
  toLocalDatetimeValue,
} from './helpers';

// ── Data fetcher ──

async function fetchContests(apiClient: ApiClient, params: ServerTableParams) {
  const { data, error } = await apiClient.GET('/contests', {
    params: {
      query: {
        page: params.page,
        per_page: params.per_page,
        search: params.search,
        sort_by: params.sort_by,
        sort_order: params.sort_order,
      },
    },
  });
  if (error) throw error;
  return { data: data.data, pagination: data.pagination };
}

type ParsedBulkUser = {
  username: string;
  password?: string;
};

type UserPreviewItem = {
  id: number;
  username: string;
  role: string;
  created_at: string;
};

function normalizeBulkUsers(input: unknown): ParsedBulkUser[] {
  if (!Array.isArray(input)) {
    throw new Error('admin.bulkParticipantsInvalidJson');
  }

  const users: ParsedBulkUser[] = [];
  const seen = new Set<string>();

  for (const item of input) {
    let username = '';
    let password: string | undefined;

    if (typeof item === 'string') {
      username = item.trim();
    } else if (item && typeof item === 'object') {
      const record = item as { username?: unknown; password?: unknown };
      username =
        typeof record.username === 'string' ? record.username.trim() : '';
      if (typeof record.password === 'string' && record.password.trim()) {
        password = record.password;
      }
    }

    if (!username) {
      throw new Error('admin.bulkParticipantsInvalidUsername');
    }

    if (username.length > 32 || !/^[A-Za-z0-9_]+$/.test(username)) {
      throw new Error('admin.bulkParticipantsInvalidUsername');
    }

    const key = username.toLowerCase();
    if (seen.has(key)) {
      throw new Error('admin.bulkParticipantsDuplicate');
    }
    seen.add(key);

    users.push({ username, password });
  }

  if (users.length === 0) {
    throw new Error('admin.bulkParticipantsEmpty');
  }

  return users;
}

// ── Problem Preview Dialog ──

function ProblemPreviewDialog({
  problemId,
  open,
  onOpenChange,
}: {
  problemId: number;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const apiClient = useApiClient();
  const { data, isLoading } = useQuery({
    queryKey: ['problem-preview', problemId],
    queryFn: async () => {
      const { data, error } = await apiClient.GET('/problems/{id}', {
        params: { path: { id: problemId } },
      });
      if (error) throw error;
      return data;
    },
    enabled: open,
  });

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-3xl max-h-[85vh] flex flex-col">
        <DialogHeader>
          <DialogTitle>
            {data ? `#${data.id} — ${data.title}` : 'Loading...'}
          </DialogTitle>
          {data && (
            <DialogDescription>
              {data.time_limit}ms · {(data.memory_limit / 1024).toFixed(0)}MB
            </DialogDescription>
          )}
        </DialogHeader>
        <div className="overflow-y-auto flex-1 pr-2">
          {isLoading ? (
            <div className="py-12 text-center text-muted-foreground">
              Loading...
            </div>
          ) : data ? (
            <Markdown>{data.content}</Markdown>
          ) : null}
        </div>
      </DialogContent>
    </Dialog>
  );
}

// ── Contest Form Dialog ──

export function ContestFormDialog({
  contest,
  open,
  onOpenChange,
}: {
  contest?: ContestListItem;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const isEdit = !!contest;

  const [title, setTitle] = useState('');
  const [description, setDescription] = useState('');
  const [startTime, setStartTime] = useState('');
  const [endTime, setEndTime] = useState('');
  const [isPublic, setIsPublic] = useState(false);
  const [submissionsVisible, setSubmissionsVisible] = useState(false);
  const [showCompileOutput, setShowCompileOutput] = useState(true);
  const [showParticipantsList, setShowParticipantsList] = useState(true);
  const [contestType, setContestType] = useState('');
  const [loading, setLoading] = useState(false);
  const [loadingData, setLoadingData] = useState(false);
  const [message, setMessage] = useState<{
    type: 'success' | 'error';
    text: string;
  } | null>(null);
  const apiClient = useApiClient();
  const { data: registries } = useRegistries();

  useEffect(() => {
    if (!open) return;
    setMessage(null);
    if (contest) {
      setLoadingData(true);
      apiClient
        .GET('/contests/{id}', { params: { path: { id: contest.id } } })
        .then(({ data, error }) => {
          setLoadingData(false);
          if (error || !data) return;
          setTitle(data.title);
          setDescription(data.description);
          setStartTime(toLocalDatetimeValue(data.start_time));
          setEndTime(toLocalDatetimeValue(data.end_time));
          setIsPublic(data.is_public);
          setSubmissionsVisible(data.submissions_visible);
          setShowCompileOutput(data.show_compile_output);
          setShowParticipantsList(data.show_participants_list);
          setContestType(data.contest_type ?? '');
        });
    } else {
      setTitle('');
      setDescription('');
      setStartTime('');
      setEndTime('');
      setIsPublic(false);
      setSubmissionsVisible(false);
      setShowCompileOutput(true);
      setShowParticipantsList(true);
      setContestType('');
    }
  }, [apiClient, open, contest]);

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    setLoading(true);
    setMessage(null);

    const body = {
      title,
      description,
      start_time: new Date(startTime).toISOString(),
      end_time: new Date(endTime).toISOString(),
      is_public: isPublic,
      submissions_visible: submissionsVisible,
      show_compile_output: showCompileOutput,
      show_participants_list: showParticipantsList,
      contest_type: contestType || undefined,
    };

    const result = isEdit
      ? await apiClient.PATCH('/contests/{id}', {
          params: { path: { id: contest!.id } },
          body,
        })
      : await apiClient.POST('/contests', { body });

    setLoading(false);
    if (result.error) {
      setMessage({
        type: 'error',
        text: isEdit ? t('admin.editError') : t('admin.createError'),
      });
    } else {
      queryClient.invalidateQueries({ queryKey: ['admin-contests'] });
      onOpenChange(false);
    }
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-2xl max-h-[90vh] overflow-y-auto">
        <DialogHeader>
          <DialogTitle>
            {isEdit ? t('admin.editContest') : t('admin.createContest')}
          </DialogTitle>
          <DialogDescription>
            {isEdit ? '' : t('admin.createContestDesc')}
          </DialogDescription>
        </DialogHeader>

        {loadingData ? (
          <div className="py-8 text-center text-muted-foreground">
            Loading...
          </div>
        ) : (
          <form onSubmit={handleSubmit} className="space-y-4">
            <div className="space-y-2">
              <Label htmlFor="contest-title">{t('admin.field.title')}</Label>
              <Input
                id="contest-title"
                value={title}
                onChange={(e) => setTitle(e.target.value)}
                required
                maxLength={256}
                placeholder="Weekly Contest #42"
              />
            </div>

            <div className="space-y-2">
              <Label htmlFor="contest-description">
                {t('admin.field.description')}
              </Label>
              <Textarea
                id="contest-description"
                value={description}
                onChange={(e) => setDescription(e.target.value)}
                required
                rows={4}
                placeholder="Contest description (Markdown supported)"
              />
            </div>

            <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
              <div className="space-y-2">
                <Label htmlFor="contest-start">
                  {t('admin.field.startTime')}
                </Label>
                <Input
                  id="contest-start"
                  type="datetime-local"
                  value={startTime}
                  onChange={(e) => setStartTime(e.target.value)}
                  required
                />
              </div>
              <div className="space-y-2">
                <Label htmlFor="contest-end">{t('admin.field.endTime')}</Label>
                <Input
                  id="contest-end"
                  type="datetime-local"
                  value={endTime}
                  onChange={(e) => setEndTime(e.target.value)}
                  required
                />
              </div>
            </div>

            <div className="space-y-2">
              <Label htmlFor="contest-type">
                {t('admin.field.contestType')}
              </Label>
              <select
                id="contest-type"
                value={contestType}
                onChange={(e) => setContestType(e.target.value)}
                className="flex h-9 w-full rounded-md border border-input bg-transparent px-3 py-1 text-sm shadow-sm transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
              >
                <option value="">{t('admin.field.contestTypeNone')}</option>
                {(registries?.contest_types ?? []).map((opt) => (
                  <option key={opt} value={opt}>
                    {opt}
                  </option>
                ))}
              </select>
            </div>

            <Separator />

            <div className="space-y-3">
              <Label className="text-sm text-muted-foreground">
                {t('admin.field.options')}
              </Label>
              <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
                <SwitchField
                  id="contest-public"
                  label={t('admin.field.isPublic')}
                  checked={isPublic}
                  onCheckedChange={setIsPublic}
                />
                <SwitchField
                  id="contest-submissions"
                  label={t('admin.field.submissionsVisible')}
                  checked={submissionsVisible}
                  onCheckedChange={setSubmissionsVisible}
                />
                <SwitchField
                  id="contest-compile"
                  label={t('admin.field.showCompileOutput')}
                  checked={showCompileOutput}
                  onCheckedChange={setShowCompileOutput}
                />
                <SwitchField
                  id="contest-participants"
                  label={t('admin.field.showParticipantsList')}
                  checked={showParticipantsList}
                  onCheckedChange={setShowParticipantsList}
                />
              </div>
            </div>

            {message && (
              <div
                className={`rounded-md px-4 py-3 text-sm ${message.type === 'success' ? 'bg-green-500/10 text-green-500 border border-green-500/20' : 'bg-destructive/10 text-destructive border border-destructive/20'}`}
              >
                {message.text}
              </div>
            )}

            <DialogFooter>
              <Button type="submit" disabled={loading}>
                {loading
                  ? t('admin.saving')
                  : isEdit
                    ? t('admin.edit')
                    : t('admin.createContest')}
              </Button>
            </DialogFooter>
          </form>
        )}
      </DialogContent>
    </Dialog>
  );
}

// ── Contest Problems Dialog ──

function nextLabel(usedLabels: Set<string>): string {
  // A-Z, then AA-AZ, BA-BZ, ..., ZZ (max 702)
  for (let i = 0; i < 26; i++) {
    const label = String.fromCharCode(65 + i);
    if (!usedLabels.has(label)) return label;
  }
  for (let i = 0; i < 26; i++) {
    for (let j = 0; j < 26; j++) {
      const label = String.fromCharCode(65 + i) + String.fromCharCode(65 + j);
      if (!usedLabels.has(label)) return label;
    }
  }
  return '';
}

export function ContestProblemsDialog({
  contest,
  open,
  onOpenChange,
}: {
  contest: ContestListItem;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const contestProblemsKey = ['contest-problems', contest.id];
  const apiClient = useApiClient();
  const hasContestProblemConfig = useHasConfigSchemas('contest_problem');

  const { data: contestProblems = [], isLoading: loadingContestProblems } =
    useQuery({
      queryKey: contestProblemsKey,
      queryFn: async () => {
        const { data, error } = await apiClient.GET('/contests/{id}/problems', {
          params: { path: { id: contest.id } },
        });
        if (error) throw error;
        return data;
      },
      enabled: open,
    });

  const { data: allProblems = [], isLoading: loadingAllProblems } = useQuery({
    queryKey: ['all-problems-for-contest', contest.id],
    queryFn: async () => {
      const { data, error } = await apiClient.GET('/problems', {
        params: { query: { page: 1, per_page: 200 } },
      });
      if (error) throw error;
      return data.data;
    },
    enabled: open,
  });

  const [search, setSearch] = useState('');
  const [addingId, setAddingId] = useState<number | null>(null);
  const [errorMsg, setErrorMsg] = useState('');
  const [previewProblemId, setPreviewProblemId] = useState<number | null>(null);
  const [configCPOpen, setConfigCPOpen] = useState(false);
  const [configProblemId, setConfigProblemId] = useState<number | null>(null);

  useEffect(() => {
    if (open) {
      setSearch('');
      setErrorMsg('');
      setPreviewProblemId(null);
      setConfigCPOpen(false);
      setConfigProblemId(null);
    }
  }, [open]);

  const addedProblemIds = new Set(
    contestProblems.map((p: ContestProblemItem) => p.problem_id),
  );
  const usedLabels = new Set(
    contestProblems.map((p: ContestProblemItem) => p.label),
  );

  const filteredProblems = allProblems
    .filter(
      (p: ProblemListItem) =>
        !search ||
        p.title.toLowerCase().includes(search.toLowerCase()) ||
        String(p.id).includes(search),
    )
    .sort((a: ProblemListItem, b: ProblemListItem) => {
      const aAdded = addedProblemIds.has(a.id) ? 1 : 0;
      const bAdded = addedProblemIds.has(b.id) ? 1 : 0;
      if (aAdded !== bAdded) return aAdded - bAdded;
      return a.id - b.id;
    });

  async function handleAdd(problemId: number) {
    const autoLabel = nextLabel(usedLabels);
    if (!autoLabel) return;
    setAddingId(problemId);
    setErrorMsg('');
    const { error: apiError } = await apiClient.POST(
      '/contests/{id}/problems',
      {
        params: { path: { id: contest.id } },
        body: { problem_id: problemId, label: autoLabel },
      },
    );
    setAddingId(null);
    if (apiError) {
      setErrorMsg(t('admin.addProblemError'));
    } else {
      queryClient.invalidateQueries({ queryKey: contestProblemsKey });
    }
  }

  async function handleRemove(problemId: number) {
    if (!window.confirm(t('admin.deleteConfirm'))) return;
    const { error: apiError } = await apiClient.DELETE(
      '/contests/{id}/problems/{problem_id}',
      {
        params: { path: { id: contest.id, problem_id: problemId } },
      },
    );
    if (!apiError) {
      queryClient.invalidateQueries({ queryKey: contestProblemsKey });
    }
  }

  const isLoading = loadingContestProblems || loadingAllProblems;

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-2xl max-h-[90vh] flex flex-col">
        <DialogHeader>
          <DialogTitle>{t('admin.contestProblems')}</DialogTitle>
          <DialogDescription>{contest.title}</DialogDescription>
        </DialogHeader>

        {!isLoading && contestProblems.length > 0 && (
          <div className="rounded-md border overflow-y-auto max-h-60">
            <table className="w-full text-sm">
              <thead>
                <tr className="border-b bg-muted/40">
                  <th className="px-3 py-2 text-left font-medium text-foreground/80 w-16">
                    {t('admin.field.label')}
                  </th>
                  <th className="px-3 py-2 text-left font-medium text-foreground/80">
                    {t('admin.field.title')}
                  </th>
                  <th className="px-3 py-2 text-right font-medium text-foreground/80 w-20" />
                </tr>
              </thead>
              <tbody>
                {contestProblems.map((p: ContestProblemItem) => (
                  <tr
                    key={p.problem_id}
                    className="border-b last:border-0 hover:bg-muted/30"
                  >
                    <td className="px-3 py-2 font-medium">{p.label}</td>
                    <td className="px-3 py-2">{p.problem_title}</td>
                    <td className="px-3 py-2 text-right">
                      <div className="flex items-center justify-end gap-1">
                        <Button
                          variant="ghost"
                          size="icon"
                          className="h-7 w-7"
                          onClick={() => setPreviewProblemId(p.problem_id)}
                        >
                          <Eye className="h-3.5 w-3.5" />
                        </Button>
                        {hasContestProblemConfig && (
                          <Button
                            variant="ghost"
                            size="icon"
                            className="h-7 w-7"
                            onClick={() => {
                              setConfigProblemId(p.problem_id);
                              setConfigCPOpen(true);
                            }}
                          >
                            <Settings className="h-3.5 w-3.5" />
                          </Button>
                        )}
                        <Button
                          variant="ghost"
                          size="icon"
                          className="h-7 w-7 text-destructive hover:text-destructive"
                          onClick={() => handleRemove(p.problem_id)}
                        >
                          <Trash2 className="h-3.5 w-3.5" />
                        </Button>
                      </div>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}

        <Separator />

        <div className="space-y-2 min-h-0 flex flex-col">
          <Label className="text-sm font-medium">
            {t('admin.availableProblems')}
          </Label>
          <div className="relative">
            <Search className="absolute left-2.5 top-1/2 -translate-y-1/2 h-3.5 w-3.5 text-muted-foreground" />
            <Input
              placeholder={t('problems.searchPlaceholder')}
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              className="pl-8 h-8 text-sm"
            />
          </div>

          {errorMsg && <p className="text-sm text-destructive">{errorMsg}</p>}

          <div className="overflow-y-auto max-h-64 rounded-md border">
            {isLoading ? (
              <div className="py-8 text-center text-muted-foreground">
                Loading...
              </div>
            ) : filteredProblems.length === 0 ? (
              <div className="py-8 text-center text-sm text-muted-foreground">
                {t('problems.empty')}
              </div>
            ) : (
              <table className="w-full text-sm">
                <tbody>
                  {filteredProblems.map((p: ProblemListItem) => {
                    const isAdded = addedProblemIds.has(p.id);
                    const contestProblem = contestProblems.find(
                      (cp: ContestProblemItem) => cp.problem_id === p.id,
                    );
                    return (
                      <tr
                        key={p.id}
                        className={`border-b last:border-0 ${isAdded ? 'opacity-50' : 'hover:bg-muted/30'}`}
                      >
                        <td className="px-3 py-2 text-muted-foreground w-12">
                          #{p.id}
                        </td>
                        <td className="px-3 py-2">
                          <button
                            type="button"
                            className={`text-left hover:underline ${isAdded ? 'text-muted-foreground' : 'font-medium'}`}
                            onClick={() => setPreviewProblemId(p.id)}
                          >
                            {p.title}
                          </button>
                        </td>
                        <td className="px-3 py-2 text-right w-24">
                          {isAdded ? (
                            <Badge variant="secondary" className="text-xs">
                              <Check className="h-3 w-3 mr-1" />
                              {contestProblem?.label}
                            </Badge>
                          ) : (
                            <Button
                              variant="outline"
                              size="sm"
                              className="h-7 text-xs"
                              disabled={addingId === p.id}
                              onClick={() => handleAdd(p.id)}
                            >
                              <Plus className="h-3 w-3 mr-1" />
                              {addingId === p.id
                                ? t('admin.adding')
                                : t('admin.addProblem')}
                            </Button>
                          )}
                        </td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            )}
          </div>
        </div>

        {previewProblemId !== null && (
          <ProblemPreviewDialog
            problemId={previewProblemId}
            open={previewProblemId !== null}
            onOpenChange={(v) => {
              if (!v) setPreviewProblemId(null);
            }}
          />
        )}
        {configProblemId !== null && (
          <ResourceConfigDialog
            scope={{
              scope: 'contest_problem',
              contestId: contest.id,
              problemId: configProblemId,
            }}
            resourceLabel={
              contestProblems.find(
                (p: ContestProblemItem) => p.problem_id === configProblemId,
              )?.problem_title ?? `Problem ${configProblemId}`
            }
            open={configCPOpen}
            onOpenChange={setConfigCPOpen}
          />
        )}
      </DialogContent>
    </Dialog>
  );
}

function BulkParticipantsDialog({
  contest,
  open,
  onOpenChange,
}: {
  contest: ContestListItem;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const { t } = useTranslation();
  const apiClient = useApiClient();
  const [jsonText, setJsonText] = useState('');
  const [loadingPreview, setLoadingPreview] = useState(false);
  const [submitting, setSubmitting] = useState(false);
  const [errorMsg, setErrorMsg] = useState('');
  const [result, setResult] = useState<{
    added: { user_id: number; username: string }[];
    created: { user_id: number; username: string; password: string }[];
    already_enrolled: { user_id: number; username: string }[];
    not_found: string[];
  } | null>(null);
  const [preview, setPreview] = useState<{
    willCreate: ParsedBulkUser[];
    willAdd: UserPreviewItem[];
    alreadyEnrolled: UserPreviewItem[];
    existingWithPassword: UserPreviewItem[];
  } | null>(null);

  useEffect(() => {
    if (!open) return;
    setJsonText('');
    setLoadingPreview(false);
    setSubmitting(false);
    setErrorMsg('');
    setPreview(null);
    setResult(null);
  }, [open]);

  async function handleReadJsonFile(e: React.ChangeEvent<HTMLInputElement>) {
    const file = e.target.files?.[0];
    if (!file) return;
    const text = await file.text();
    setJsonText(text);
    setPreview(null);
    setResult(null);
    setErrorMsg('');
    e.target.value = '';
  }

  async function handlePreview() {
    setLoadingPreview(true);
    setErrorMsg('');
    setResult(null);
    try {
      const parsed = normalizeBulkUsers(JSON.parse(jsonText));
      const [
        { data: participants, error: participantsError },
        { data: users, error: usersError },
      ] = await Promise.all([
        apiClient.GET('/contests/{id}/participants', {
          params: { path: { id: contest.id } },
        }),
        apiClient.GET('/users'),
      ]);

      if (participantsError || usersError || !participants || !users) {
        throw new Error('admin.bulkParticipantsError');
      }

      const allUsersMap = new Map<string, UserPreviewItem>();
      for (const user of users) {
        allUsersMap.set(user.username.toLowerCase(), {
          id: user.id,
          username: user.username,
          role: user.role,
          created_at: user.created_at,
        });
      }

      const enrolledMap = new Map<string, UserPreviewItem>();
      for (const participant of participants) {
        const detail = allUsersMap.get(participant.username.toLowerCase());
        enrolledMap.set(participant.username.toLowerCase(), {
          id: participant.user_id,
          username: participant.username,
          role: detail?.role ?? '-',
          created_at: detail?.created_at ?? '',
        });
      }

      const willCreate: ParsedBulkUser[] = [];
      const willAdd: UserPreviewItem[] = [];
      const alreadyEnrolled: UserPreviewItem[] = [];
      const existingWithPassword: UserPreviewItem[] = [];

      for (const item of parsed) {
        const key = item.username.toLowerCase();
        const enrolled = enrolledMap.get(key);
        if (enrolled) {
          if (item.password) {
            existingWithPassword.push(enrolled);
            continue;
          }
          alreadyEnrolled.push(enrolled);
          continue;
        }

        const existing = allUsersMap.get(key);
        if (existing) {
          if (item.password) {
            existingWithPassword.push(existing);
            continue;
          }
          willAdd.push(existing);
        } else {
          willCreate.push(item);
        }
      }

      setPreview({
        willCreate,
        willAdd,
        alreadyEnrolled,
        existingWithPassword,
      });
      if (existingWithPassword.length > 0) {
        setErrorMsg(t('admin.bulkParticipantsExistingWithPassword'));
      }
    } catch (error) {
      if (error instanceof Error && error.message.startsWith('admin.')) {
        setErrorMsg(t(error.message));
      } else {
        setErrorMsg(t('admin.bulkParticipantsInvalidJson'));
      }
      setPreview(null);
    } finally {
      setLoadingPreview(false);
    }
  }

  async function handleConfirm() {
    if (!preview || preview.willCreate.length + preview.willAdd.length === 0)
      return;
    if (preview.existingWithPassword.length > 0) {
      setErrorMsg(t('admin.bulkParticipantsExistingWithPassword'));
      return;
    }

    setSubmitting(true);
    setErrorMsg('');
    const { data, error } = await apiClient.POST(
      '/contests/{id}/participants/bulk',
      {
        params: { path: { id: contest.id } },
        body: {
          usernames: preview.willAdd.map((user) => user.username),
          create_users: preview.willCreate.map((entry) => ({
            username: entry.username,
            password: entry.password,
          })),
        },
      },
    );
    setSubmitting(false);

    if (error || !data) {
      setErrorMsg(t('admin.bulkParticipantsError'));
      return;
    }

    setResult(data);
  }

  function PreviewUserTable({
    title,
    users,
  }: {
    title: string;
    users: UserPreviewItem[];
  }) {
    if (users.length === 0) return null;
    return (
      <div className="rounded-md border overflow-hidden">
        <div className="px-3 py-2 border-b bg-muted/40 text-sm font-medium">
          {title}
        </div>
        <table className="w-full text-sm">
          <thead>
            <tr className="border-b bg-muted/20">
              <th className="px-3 py-2 text-left font-medium text-foreground/80">
                #
              </th>
              <th className="px-3 py-2 text-left font-medium text-foreground/80">
                {t('auth.username')}
              </th>
              <th className="px-3 py-2 text-left font-medium text-foreground/80">
                {t('admin.field.role')}
              </th>
            </tr>
          </thead>
          <tbody>
            {users.map((user) => (
              <tr
                key={`${title}-${user.id}-${user.username}`}
                className="border-b last:border-0"
              >
                <td className="px-3 py-2">{user.id}</td>
                <td className="px-3 py-2">{user.username}</td>
                <td className="px-3 py-2">{user.role}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    );
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-3xl max-h-[90vh] overflow-y-auto">
        <DialogHeader>
          <DialogTitle>{t('admin.bulkParticipants')}</DialogTitle>
          <DialogDescription>
            {t('admin.bulkParticipantsDesc')} · {contest.title}
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-3">
          <Label htmlFor="bulk-participants-json">
            {t('admin.bulkParticipantsJsonLabel')}
          </Label>
          <Input
            id="bulk-participants-json"
            type="file"
            accept="application/json"
            onChange={handleReadJsonFile}
          />
          <Textarea
            value={jsonText}
            onChange={(e) => {
              setJsonText(e.target.value);
              setPreview(null);
              setResult(null);
              setErrorMsg('');
            }}
            rows={8}
            placeholder={t('admin.bulkParticipantsJsonPlaceholder')}
          />
          <Button
            variant="outline"
            onClick={handlePreview}
            disabled={loadingPreview}
          >
            <Upload className="h-4 w-4 mr-1" />
            {loadingPreview
              ? t('admin.loading')
              : t('admin.bulkParticipantsPreview')}
          </Button>
        </div>

        {errorMsg && (
          <div className="rounded-md border border-destructive/20 bg-destructive/10 px-4 py-3 text-sm text-destructive">
            {errorMsg}
          </div>
        )}

        {preview && (
          <div className="space-y-3 rounded-md border p-4">
            <Label className="text-sm font-medium">
              {t('admin.bulkParticipantsPreviewTitle')}
            </Label>
            <div className="grid grid-cols-2 gap-3 sm:grid-cols-4">
              <div className="rounded-md border p-3">
                <p className="text-xs text-muted-foreground mb-1">
                  {t('admin.bulkParticipantsWillCreate')}
                </p>
                <p className="text-lg font-semibold">
                  {preview.willCreate.length}
                </p>
              </div>
              <div className="rounded-md border p-3">
                <p className="text-xs text-muted-foreground mb-1">
                  {t('admin.bulkParticipantsWillAdd')}
                </p>
                <p className="text-lg font-semibold">
                  {preview.willAdd.length}
                </p>
              </div>
              <div className="rounded-md border p-3">
                <p className="text-xs text-muted-foreground mb-1">
                  {t('admin.bulkParticipantsAlreadyEnrolled')}
                </p>
                <p className="text-lg font-semibold">
                  {preview.alreadyEnrolled.length}
                </p>
              </div>
              <div className="rounded-md border p-3">
                <p className="text-xs text-muted-foreground mb-1">
                  {t('admin.bulkParticipantsExistingWithPasswordList')}
                </p>
                <p
                  className={`text-lg font-semibold ${preview.existingWithPassword.length > 0 ? 'text-destructive' : ''}`}
                >
                  {preview.existingWithPassword.length}
                </p>
              </div>
            </div>

            {preview.willCreate.length > 0 && (
              <div className="rounded-md border overflow-hidden">
                <div className="px-3 py-2 border-b bg-muted/40 text-sm font-medium">
                  {t('admin.bulkParticipantsWillCreateList')}
                </div>
                <table className="w-full text-sm">
                  <thead>
                    <tr className="border-b bg-muted/20">
                      <th className="px-3 py-2 text-left font-medium text-foreground/80">
                        {t('auth.username')}
                      </th>
                      <th className="px-3 py-2 text-left font-medium text-foreground/80">
                        {t('admin.field.password')}
                      </th>
                    </tr>
                  </thead>
                  <tbody>
                    {preview.willCreate.map((entry) => (
                      <tr
                        key={`create-${entry.username}`}
                        className="border-b last:border-0"
                      >
                        <td className="px-3 py-2">{entry.username}</td>
                        <td className="px-3 py-2 text-xs text-muted-foreground">
                          {entry.password ??
                            t('admin.bulkParticipantsAutoPassword')}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            )}

            <PreviewUserTable
              title={t('admin.bulkParticipantsWillAddList')}
              users={preview.willAdd}
            />
            <PreviewUserTable
              title={t('admin.bulkParticipantsAlreadyEnrolledList')}
              users={preview.alreadyEnrolled}
            />
            <PreviewUserTable
              title={t('admin.bulkParticipantsExistingWithPasswordList')}
              users={preview.existingWithPassword}
            />

            <DialogFooter>
              <Button
                onClick={handleConfirm}
                disabled={
                  submitting ||
                  preview.willCreate.length + preview.willAdd.length === 0 ||
                  preview.existingWithPassword.length > 0
                }
              >
                {submitting
                  ? t('admin.saving')
                  : t('admin.bulkParticipantsConfirm')}
              </Button>
            </DialogFooter>
          </div>
        )}

        {result && (
          <div className="space-y-3 rounded-md border p-4">
            <Label className="text-sm font-medium">
              {t('admin.bulkParticipantsResultTitle')}
            </Label>
            <div className="grid grid-cols-2 gap-3 sm:grid-cols-4">
              <div className="rounded-md border p-3">
                <p className="text-xs text-muted-foreground mb-1">
                  {t('admin.bulkParticipantsCreated')}
                </p>
                <p className="text-lg font-semibold">{result.created.length}</p>
              </div>
              <div className="rounded-md border p-3">
                <p className="text-xs text-muted-foreground mb-1">
                  {t('admin.bulkParticipantsAdded')}
                </p>
                <p className="text-lg font-semibold">{result.added.length}</p>
              </div>
              <div className="rounded-md border p-3">
                <p className="text-xs text-muted-foreground mb-1">
                  {t('admin.bulkParticipantsSkipped')}
                </p>
                <p className="text-lg font-semibold">
                  {result.already_enrolled.length}
                </p>
              </div>
              <div className="rounded-md border p-3">
                <p className="text-xs text-muted-foreground mb-1">
                  {t('admin.bulkParticipantsNotFound')}
                </p>
                <p className="text-lg font-semibold">
                  {result.not_found.length}
                </p>
              </div>
            </div>

            {result.created.length > 0 && (
              <div className="rounded-md border overflow-hidden">
                <table className="w-full text-sm">
                  <thead>
                    <tr className="border-b bg-muted/40">
                      <th className="px-3 py-2 text-left font-medium text-foreground/80">
                        {t('auth.username')}
                      </th>
                      <th className="px-3 py-2 text-left font-medium text-foreground/80">
                        {t('admin.field.password')}
                      </th>
                    </tr>
                  </thead>
                  <tbody>
                    {result.created.map((entry) => (
                      <tr
                        key={entry.user_id}
                        className="border-b last:border-0"
                      >
                        <td className="px-3 py-2">{entry.username}</td>
                        <td className="px-3 py-2 font-mono text-xs">
                          {entry.password}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            )}

            {result.added.length > 0 && (
              <div className="rounded-md border overflow-hidden">
                <div className="px-3 py-2 border-b bg-muted/40 text-sm font-medium">
                  {t('admin.bulkParticipantsAddedList')}
                </div>
                <table className="w-full text-sm">
                  <thead>
                    <tr className="border-b bg-muted/20">
                      <th className="px-3 py-2 text-left font-medium text-foreground/80">
                        #
                      </th>
                      <th className="px-3 py-2 text-left font-medium text-foreground/80">
                        {t('auth.username')}
                      </th>
                    </tr>
                  </thead>
                  <tbody>
                    {result.added.map((entry) => (
                      <tr
                        key={entry.user_id}
                        className="border-b last:border-0"
                      >
                        <td className="px-3 py-2">{entry.user_id}</td>
                        <td className="px-3 py-2">{entry.username}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            )}
          </div>
        )}
      </DialogContent>
    </Dialog>
  );
}

// ── Column hook ──

function useContestColumns({
  onEdit,
  onDelete,
  onManageProblems,
  onBulkParticipants,
  onConfigure,
}: {
  onEdit: (contest: ContestListItem) => void;
  onDelete: (contest: ContestListItem) => void;
  onManageProblems: (contest: ContestListItem) => void;
  onBulkParticipants: (contest: ContestListItem) => void;
  onConfigure?: (contest: ContestListItem) => void;
}): DataTableColumn<ContestListItem>[] {
  const { t, locale } = useTranslation();
  return [
    { accessorKey: 'id', header: '#', size: 60 },
    {
      accessorKey: 'title',
      header: t('admin.field.title'),
      sortKey: 'title',
      cell: ({ row }) => (
        <Link
          to={`/contests/${row.original.id}`}
          className="font-medium hover:text-primary hover:underline"
        >
          {row.original.title}
        </Link>
      ),
    },
    {
      id: 'status',
      header: t('contests.status'),
      size: 110,
      cell: ({ row }) => {
        const { label, variant } = getContestStatus(
          row.original.start_time,
          row.original.end_time,
          t,
        );
        return <Badge variant={variant}>{label}</Badge>;
      },
    },
    {
      id: 'contest_type',
      header: t('admin.field.contestType'),
      size: 120,
      cell: ({ row }) =>
        row.original.contest_type ? (
          <Badge variant="outline">{row.original.contest_type}</Badge>
        ) : (
          <span className="text-muted-foreground">—</span>
        ),
    },
    {
      id: 'visibility',
      header: '',
      size: 40,
      cell: ({ row }) =>
        row.original.is_public ? (
          <Eye className="h-3.5 w-3.5 text-muted-foreground" />
        ) : (
          <EyeOff className="h-3.5 w-3.5 text-muted-foreground" />
        ),
    },
    {
      accessorKey: 'start_time',
      header: t('contests.startTime'),
      size: 180,
      sortKey: 'start_time',
      cell: ({ row }) => (
        <span className="text-muted-foreground">
          {formatDateTime(row.original.start_time, locale)}
        </span>
      ),
    },
    {
      accessorKey: 'end_time',
      header: t('contests.endTime'),
      size: 180,
      cell: ({ row }) => (
        <span className="text-muted-foreground">
          {formatDateTime(row.original.end_time, locale)}
        </span>
      ),
    },
    {
      id: 'actions',
      header: '',
      size: 50,
      cell: ({ row }) => (
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button variant="ghost" size="icon" className="h-7 w-7">
              <MoreHorizontal className="h-4 w-4" />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end">
            <DropdownMenuItem onClick={() => onManageProblems(row.original)}>
              <List className="h-4 w-4" />
              {t('admin.contestProblems')}
            </DropdownMenuItem>
            <DropdownMenuItem onClick={() => onBulkParticipants(row.original)}>
              <UserPlus className="h-4 w-4" />
              {t('admin.bulkParticipantsAction')}
            </DropdownMenuItem>
            {onConfigure && (
              <DropdownMenuItem onClick={() => onConfigure(row.original)}>
                <Settings className="h-4 w-4" />
                {t('admin.configure')}
              </DropdownMenuItem>
            )}
            <DropdownMenuItem onClick={() => onEdit(row.original)}>
              <Pencil className="h-4 w-4" />
              {t('admin.edit')}
            </DropdownMenuItem>
            <DropdownMenuSeparator />
            <DropdownMenuItem
              className="text-destructive focus:text-destructive"
              onClick={() => onDelete(row.original)}
            >
              <Trash2 className="h-4 w-4" />
              {t('admin.delete')}
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      ),
    },
  ];
}

// ── Contests Tab ──

export function AdminContestsTab() {
  const { t } = useTranslation();
  const apiClient = useApiClient();
  const queryClient = useQueryClient();
  const hasContestConfig = useHasConfigSchemas('contest');

  const [contestDialogOpen, setContestDialogOpen] = useState(false);
  const [editingContest, setEditingContest] = useState<
    ContestListItem | undefined
  >();
  const [contestProblemsDialogOpen, setContestProblemsDialogOpen] =
    useState(false);
  const [managingContest, setManagingContest] = useState<
    ContestListItem | undefined
  >();
  const [bulkParticipantsDialogOpen, setBulkParticipantsDialogOpen] =
    useState(false);
  const [bulkParticipantsContest, setBulkParticipantsContest] = useState<
    ContestListItem | undefined
  >();
  const [configDialogOpen, setConfigDialogOpen] = useState(false);
  const [configContest, setConfigContest] = useState<
    ContestListItem | undefined
  >();

  function handleCreateContest() {
    setEditingContest(undefined);
    setContestDialogOpen(true);
  }

  function handleEditContest(contest: ContestListItem) {
    setEditingContest(contest);
    setContestDialogOpen(true);
  }

  function handleManageProblems(contest: ContestListItem) {
    setManagingContest(contest);
    setContestProblemsDialogOpen(true);
  }

  function handleBulkParticipants(contest: ContestListItem) {
    setBulkParticipantsContest(contest);
    setBulkParticipantsDialogOpen(true);
  }

  function handleConfigure(contest: ContestListItem) {
    setConfigContest(contest);
    setConfigDialogOpen(true);
  }

  async function handleDeleteContest(contest: ContestListItem) {
    if (!window.confirm(t('admin.deleteConfirm'))) return;
    const { error } = await apiClient.DELETE('/contests/{id}', {
      params: { path: { id: contest.id } },
    });
    if (!error) {
      queryClient.invalidateQueries({ queryKey: ['admin-contests'] });
    }
  }

  const columns = useContestColumns({
    onEdit: handleEditContest,
    onDelete: handleDeleteContest,
    onManageProblems: handleManageProblems,
    onBulkParticipants: handleBulkParticipants,
    onConfigure: hasContestConfig ? handleConfigure : undefined,
  });

  return (
    <>
      <DataTable
        columns={columns}
        queryKey={['admin-contests']}
        fetchFn={fetchContests}
        searchable
        searchPlaceholder={t('contests.searchPlaceholder')}
        defaultPerPage={20}
        defaultSortBy="created_at"
        defaultSortOrder="desc"
        emptyMessage={t('admin.noContests')}
        toolbar={
          <Button size="sm" onClick={handleCreateContest}>
            <Plus className="h-4 w-4 mr-1" />
            {t('admin.createContest')}
          </Button>
        }
      />
      <ContestFormDialog
        contest={editingContest}
        open={contestDialogOpen}
        onOpenChange={setContestDialogOpen}
      />
      {managingContest && (
        <ContestProblemsDialog
          contest={managingContest}
          open={contestProblemsDialogOpen}
          onOpenChange={setContestProblemsDialogOpen}
        />
      )}
      {bulkParticipantsContest && (
        <BulkParticipantsDialog
          contest={bulkParticipantsContest}
          open={bulkParticipantsDialogOpen}
          onOpenChange={setBulkParticipantsDialogOpen}
        />
      )}
      {configContest && (
        <ResourceConfigDialog
          scope={{ scope: 'contest', contestId: configContest.id }}
          resourceLabel={configContest.title}
          open={configDialogOpen}
          onOpenChange={setConfigDialogOpen}
        />
      )}
    </>
  );
}
