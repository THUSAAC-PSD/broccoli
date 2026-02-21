import { useEffect, useState } from 'react';

import { useTranslation } from '@broccoli/sdk/i18n';
import { useApiClient, type ApiClient } from '@broccoli/sdk/api';
import type {
  ContestListItem,
  ContestProblemItem,
  ProblemListItem,
} from '@broccoli/sdk';
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
  Trash2,
} from 'lucide-react';

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
import type { ServerTableParams } from '@/hooks/use-server-table';

import {
  SwitchField,
  getContestStatus,
  formatDateTime,
  toLocalDatetimeValue,
} from './helpers';

// ── Data fetcher ──

export async function fetchContests(
  apiClient: ApiClient,
  params: ServerTableParams,
) {
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
  const [loading, setLoading] = useState(false);
  const [loadingData, setLoadingData] = useState(false);
  const [message, setMessage] = useState<{
    type: 'success' | 'error';
    text: string;
  } | null>(null);
  const apiClient = useApiClient();

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
  for (let i = 0; i < 26; i++) {
    const ch = String.fromCharCode(65 + i);
    if (!usedLabels.has(ch)) return ch;
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

  useEffect(() => {
    if (open) {
      setSearch('');
      setErrorMsg('');
      setPreviewProblemId(null);
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
          <div className="rounded-md border">
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
      </DialogContent>
    </Dialog>
  );
}

// ── Column hook ──

export function useContestColumns({
  onEdit,
  onDelete,
  onManageProblems,
}: {
  onEdit: (contest: ContestListItem) => void;
  onDelete: (contest: ContestListItem) => void;
  onManageProblems: (contest: ContestListItem) => void;
}): DataTableColumn<ContestListItem>[] {
  const { t } = useTranslation();
  return [
    { accessorKey: 'id', header: '#', size: 60 },
    {
      accessorKey: 'title',
      header: t('admin.field.title'),
      sortKey: 'title',
      cell: ({ row }) => (
        <span className="font-medium">{row.original.title}</span>
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
          {formatDateTime(row.original.start_time)}
        </span>
      ),
    },
    {
      accessorKey: 'end_time',
      header: t('contests.endTime'),
      size: 180,
      cell: ({ row }) => (
        <span className="text-muted-foreground">
          {formatDateTime(row.original.end_time)}
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

  const [contestDialogOpen, setContestDialogOpen] = useState(false);
  const [editingContest, setEditingContest] = useState<
    ContestListItem | undefined
  >();
  const [contestProblemsDialogOpen, setContestProblemsDialogOpen] =
    useState(false);
  const [managingContest, setManagingContest] = useState<
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
    </>
  );
}
