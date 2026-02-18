import { useEffect, useState } from 'react';

import { useTranslation } from '@broccoli/sdk/i18n';
import { useApiClient, type ApiClient } from '@broccoli/sdk/api';
import type { ProblemListItem } from '@broccoli/sdk';
import { useQueryClient } from '@tanstack/react-query';
import { MoreHorizontal, Pencil, Plus, Trash2 } from 'lucide-react';

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

import { SwitchField, formatDateTime } from './helpers';

// ── Data fetcher ──

export async function fetchProblems(
  apiClient: ApiClient,
  params: ServerTableParams,
) {
  const { data, error } = await apiClient.GET('/problems', {
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

// ── Problem Form Dialog ──

export function ProblemFormDialog({
  problem,
  open,
  onOpenChange,
}: {
  problem?: ProblemListItem;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const isEdit = !!problem;

  const [title, setTitle] = useState('');
  const [content, setContent] = useState('');
  const [timeLimit, setTimeLimit] = useState(1000);
  const [memoryLimit, setMemoryLimit] = useState(262144);
  const [showTestDetails, setShowTestDetails] = useState(false);
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
    if (problem) {
      setLoadingData(true);
      apiClient
        .GET('/problems/{id}', { params: { path: { id: problem.id } } })
        .then(({ data, error }) => {
          setLoadingData(false);
          if (error || !data) return;
          setTitle(data.title);
          setContent(data.content);
          setTimeLimit(data.time_limit);
          setMemoryLimit(data.memory_limit);
          setShowTestDetails(data.show_test_details);
        });
    } else {
      setTitle('');
      setContent('');
      setTimeLimit(1000);
      setMemoryLimit(262144);
      setShowTestDetails(false);
    }
  }, [apiClient, open, problem]);

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    setLoading(true);
    setMessage(null);

    const body = {
      title,
      content,
      time_limit: timeLimit,
      memory_limit: memoryLimit,
      show_test_details: showTestDetails,
    };

    const result = isEdit
      ? await apiClient.PATCH('/problems/{id}', {
          params: { path: { id: problem!.id } },
          body,
        })
      : await apiClient.POST('/problems', { body });

    setLoading(false);
    if (result.error) {
      setMessage({
        type: 'error',
        text: isEdit ? t('admin.editError') : t('admin.createError'),
      });
    } else {
      queryClient.invalidateQueries({ queryKey: ['admin-problems'] });
      onOpenChange(false);
    }
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-2xl max-h-[90vh] overflow-y-auto">
        <DialogHeader>
          <DialogTitle>
            {isEdit ? t('admin.editProblem') : t('admin.createProblem')}
          </DialogTitle>
          <DialogDescription>
            {isEdit ? '' : t('admin.createProblemDesc')}
          </DialogDescription>
        </DialogHeader>

        {loadingData ? (
          <div className="py-8 text-center text-muted-foreground">
            Loading...
          </div>
        ) : (
          <form onSubmit={handleSubmit} className="space-y-4">
            <div className="space-y-2">
              <Label htmlFor="problem-title">{t('admin.field.title')}</Label>
              <Input
                id="problem-title"
                value={title}
                onChange={(e) => setTitle(e.target.value)}
                required
                maxLength={256}
                placeholder="Two Sum"
              />
            </div>

            <div className="space-y-2">
              <Label htmlFor="problem-content">
                {t('admin.field.content')}
              </Label>
              <Textarea
                id="problem-content"
                value={content}
                onChange={(e) => setContent(e.target.value)}
                required
                rows={8}
                placeholder="Problem statement (Markdown supported)"
              />
            </div>

            <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
              <div className="space-y-2">
                <Label htmlFor="problem-time">
                  {t('admin.field.timeLimit')}
                </Label>
                <Input
                  id="problem-time"
                  type="number"
                  min={1}
                  max={30000}
                  value={timeLimit}
                  onChange={(e) => setTimeLimit(Number(e.target.value))}
                  required
                />
              </div>
              <div className="space-y-2">
                <Label htmlFor="problem-memory">
                  {t('admin.field.memoryLimit')}
                </Label>
                <Input
                  id="problem-memory"
                  type="number"
                  min={1}
                  max={1048576}
                  value={memoryLimit}
                  onChange={(e) => setMemoryLimit(Number(e.target.value))}
                  required
                />
              </div>
            </div>

            <Separator />

            <div className="space-y-3">
              <Label className="text-sm text-muted-foreground">
                {t('admin.field.options')}
              </Label>
              <SwitchField
                id="problem-test-details"
                label={t('admin.field.showTestDetails')}
                checked={showTestDetails}
                onCheckedChange={setShowTestDetails}
              />
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
                    : t('admin.createProblem')}
              </Button>
            </DialogFooter>
          </form>
        )}
      </DialogContent>
    </Dialog>
  );
}

// ── Column hook ──

export function useProblemColumns({
  onEdit,
  onDelete,
}: {
  onEdit: (problem: ProblemListItem) => void;
  onDelete: (problem: ProblemListItem) => void;
}): DataTableColumn<ProblemListItem>[] {
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
      accessorKey: 'time_limit',
      header: t('admin.field.timeLimit'),
      size: 120,
      cell: ({ row }) => (
        <span className="text-muted-foreground">
          {row.original.time_limit}ms
        </span>
      ),
    },
    {
      accessorKey: 'memory_limit',
      header: t('admin.field.memoryLimit'),
      size: 120,
      cell: ({ row }) => (
        <span className="text-muted-foreground">
          {(row.original.memory_limit / 1024).toFixed(0)}MB
        </span>
      ),
    },
    {
      accessorKey: 'created_at',
      header: t('admin.field.createdAt'),
      size: 180,
      sortKey: 'created_at',
      cell: ({ row }) => (
        <span className="text-muted-foreground">
          {formatDateTime(row.original.created_at)}
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

// ── Problems Tab ──

export function AdminProblemsTab() {
  const { t } = useTranslation();
  const apiClient = useApiClient();
  const queryClient = useQueryClient();

  const [problemDialogOpen, setProblemDialogOpen] = useState(false);
  const [editingProblem, setEditingProblem] = useState<
    ProblemListItem | undefined
  >();

  function handleCreateProblem() {
    setEditingProblem(undefined);
    setProblemDialogOpen(true);
  }

  function handleEditProblem(problem: ProblemListItem) {
    setEditingProblem(problem);
    setProblemDialogOpen(true);
  }

  async function handleDeleteProblem(problem: ProblemListItem) {
    if (!window.confirm(t('admin.deleteConfirm'))) return;
    const { error } = await apiClient.DELETE('/problems/{id}', {
      params: { path: { id: problem.id } },
    });
    if (!error) {
      queryClient.invalidateQueries({ queryKey: ['admin-problems'] });
    }
  }

  const columns = useProblemColumns({
    onEdit: handleEditProblem,
    onDelete: handleDeleteProblem,
  });

  return (
    <>
      <DataTable
        columns={columns}
        queryKey={['admin-problems']}
        fetchFn={fetchProblems}
        searchable
        searchPlaceholder={t('problems.searchPlaceholder')}
        defaultPerPage={20}
        defaultSortBy="created_at"
        defaultSortOrder="desc"
        emptyMessage={t('admin.noProblems')}
        toolbar={
          <Button size="sm" onClick={handleCreateProblem}>
            <Plus className="h-4 w-4 mr-1" />
            {t('admin.createProblem')}
          </Button>
        }
      />
      <ProblemFormDialog
        problem={editingProblem}
        open={problemDialogOpen}
        onOpenChange={setProblemDialogOpen}
      />
    </>
  );
}
