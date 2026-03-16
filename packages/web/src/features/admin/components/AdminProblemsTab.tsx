import { useApiClient } from '@broccoli/web-sdk/api';
import { useTranslation } from '@broccoli/web-sdk/i18n';
import type { ProblemSummary } from '@broccoli/web-sdk/problem';
import {
  Badge,
  Button,
  DataTable,
  type DataTableColumn,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@broccoli/web-sdk/ui';
import { formatDateTime } from '@broccoli/web-sdk/utils';
import { useQueryClient } from '@tanstack/react-query';
import {
  List,
  MoreHorizontal,
  Pencil,
  Plus,
  Settings,
  Trash2,
} from 'lucide-react';
import { useEffect, useState } from 'react';
import { Link } from 'react-router';
import { toast } from 'sonner';

import { ResourceConfigDialog, useHasConfigSchemas } from '@/components/config';
import {
  ProblemForm,
  type ProblemFormData,
} from '@/features/admin/components/ProblemForm';
import { TestCasesDialog } from '@/features/admin/components/TestCasesDialog';
import { fetchContestProblems } from '@/features/contest/api/fetch-contest-problems';
import { fetchProblems } from '@/features/problem/api/fetch-problems';
import { extractErrorMessage } from '@/lib/extract-error';

// ── Problem Form Dialog ──

export function ProblemFormDialog({
  problem,
  open,
  onOpenChange,
}: {
  problem?: ProblemSummary;
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
  const [problemType, setProblemType] = useState('standard');
  const [checkerFormat, setCheckerFormat] = useState('exact');
  const [defaultContestType, setDefaultContestType] = useState('standard');
  const [showTestDetails, setShowTestDetails] = useState(false);
  const [submissionFormat, setSubmissionFormat] = useState<
    Record<string, string[]>
  >({});
  const [loading, setLoading] = useState(false);
  const [loadingData, setLoadingData] = useState(false);
  const apiClient = useApiClient();

  const formData: ProblemFormData = {
    title,
    content,
    timeLimit,
    memoryLimit,
    problemType,
    checkerFormat,
    defaultContestType,
    showTestDetails,
    submissionFormat,
  };

  const handleFormChange = (data: ProblemFormData) => {
    setTitle(data.title);
    setContent(data.content);
    setTimeLimit(data.timeLimit);
    setMemoryLimit(data.memoryLimit);
    setProblemType(data.problemType);
    setCheckerFormat(data.checkerFormat);
    setDefaultContestType(data.defaultContestType);
    setShowTestDetails(data.showTestDetails);
    setSubmissionFormat(data.submissionFormat);
  };

  useEffect(() => {
    if (!open) return;
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
          setProblemType(data.problem_type);
          setCheckerFormat(data.checker_format);
          setDefaultContestType(data.default_contest_type);
          setShowTestDetails(data.show_test_details);
          setSubmissionFormat(data.submission_format ?? {});
        });
    } else {
      setTitle('');
      setContent('');
      setTimeLimit(1000);
      setMemoryLimit(262144);
      setProblemType('standard');
      setCheckerFormat('exact');
      setDefaultContestType('standard');
      setShowTestDetails(false);
      setSubmissionFormat({});
    }
  }, [apiClient, open, problem]);

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();

    if (!title.trim()) {
      toast.error(t('validation.titleRequired'));
      return;
    }
    if (!content.trim()) {
      toast.error(t('validation.contentRequired'));
      return;
    }

    setLoading(true);

    const body = {
      title,
      content,
      time_limit: timeLimit,
      memory_limit: memoryLimit,
      problem_type: problemType,
      checker_format: checkerFormat,
      default_contest_type: defaultContestType,
      show_test_details: showTestDetails,
      submission_format:
        Object.keys(submissionFormat).length > 0 ? submissionFormat : null,
    };

    const result = isEdit
      ? await apiClient.PATCH('/problems/{id}', {
          params: { path: { id: problem!.id } },
          body,
        })
      : await apiClient.POST('/problems', { body });

    setLoading(false);
    if (result.error) {
      toast.error(
        extractErrorMessage(
          result.error,
          isEdit ? t('admin.editError') : t('admin.createError'),
        ),
      );
    } else {
      toast.success(
        isEdit ? t('toast.problem.updated') : t('toast.problem.created'),
      );
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
            <ProblemForm data={formData} onChange={handleFormChange} />
            <DialogFooter>
              <Button type="submit" disabled={loading}>
                {loading
                  ? t('admin.saving')
                  : isEdit
                    ? t('admin.save')
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

function useProblemColumns({
  onEdit,
  onDelete,
  onManageTestCases,
  onConfigure,
}: {
  onEdit: (problem: ProblemSummary) => void;
  onDelete: (problem: ProblemSummary) => void;
  onManageTestCases: (problem: ProblemSummary) => void;
  onConfigure?: (problem: ProblemSummary) => void;
}): DataTableColumn<ProblemSummary>[] {
  const { t, locale } = useTranslation();
  return [
    { accessorKey: 'id', header: '#', size: 60 },
    {
      accessorKey: 'title',
      header: t('admin.field.title'),
      sortKey: 'title',
      cell: ({ row }) => (
        <Link
          to={`/problems/${row.original.id}`}
          className="font-medium hover:text-primary hover:underline"
        >
          {row.original.title}
        </Link>
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
      accessorKey: 'problem_type',
      header: t('admin.field.problemType'),
      size: 120,
      cell: ({ row }) => (
        <Badge variant="outline">{row.original.problem_type}</Badge>
      ),
    },
    {
      accessorKey: 'checker_format',
      header: t('admin.field.checkerFormat'),
      size: 120,
      cell: ({ row }) => (
        <span className="text-muted-foreground">
          {row.original.checker_format}
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
          {formatDateTime(row.original.created_at, locale)}
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
            <DropdownMenuItem onClick={() => onManageTestCases(row.original)}>
              <List className="h-4 w-4" />
              {t('admin.manageTestCases')}
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

// ── Problems Tab ──

export function AdminProblemsTab({ contestId }: { contestId?: number }) {
  const { t } = useTranslation();
  const apiClient = useApiClient();
  const queryClient = useQueryClient();
  const hasProblemConfig = useHasConfigSchemas('problem');

  const [problemDialogOpen, setProblemDialogOpen] = useState(false);
  const [editingProblem, setEditingProblem] = useState<
    ProblemSummary | undefined
  >();
  const [testCasesDialogOpen, setTestCasesDialogOpen] = useState(false);
  const [managingProblem, setManagingProblem] = useState<
    ProblemSummary | undefined
  >();
  const [configDialogOpen, setConfigDialogOpen] = useState(false);
  const [configProblem, setConfigProblem] = useState<
    ProblemSummary | undefined
  >();

  function handleCreateProblem() {
    setEditingProblem(undefined);
    setProblemDialogOpen(true);
  }

  function handleEditProblem(problem: ProblemSummary) {
    setEditingProblem(problem);
    setProblemDialogOpen(true);
  }

  function handleManageTestCases(problem: ProblemSummary) {
    setManagingProblem(problem);
    setTestCasesDialogOpen(true);
  }

  function handleConfigure(problem: ProblemSummary) {
    setConfigProblem(problem);
    setConfigDialogOpen(true);
  }

  async function handleDeleteProblem(problem: ProblemSummary) {
    if (!window.confirm(t('admin.deleteConfirm'))) return;
    const { error } = await apiClient.DELETE('/problems/{id}', {
      params: { path: { id: problem.id } },
    });
    if (error) {
      toast.error(extractErrorMessage(error, t('toast.problem.deleteError')));
    } else {
      toast.success(t('toast.problem.deleted'));
      queryClient.invalidateQueries({ queryKey: ['admin-problems'] });
    }
  }

  const columns = useProblemColumns({
    onEdit: handleEditProblem,
    onDelete: handleDeleteProblem,
    onManageTestCases: handleManageTestCases,
    onConfigure: hasProblemConfig ? handleConfigure : undefined,
  });

  return (
    <>
      <DataTable
        columns={columns}
        queryKey={['admin-problems']}
        fetchFn={(api, params) => {
          if (contestId) {
            return fetchContestProblems(api, { ...params, contestId });
          }
          return fetchProblems(api, params);
        }}
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
      {managingProblem && (
        <TestCasesDialog
          problem={managingProblem}
          open={testCasesDialogOpen}
          onOpenChange={setTestCasesDialogOpen}
        />
      )}
      {configProblem && (
        <ResourceConfigDialog
          scope={{ scope: 'problem', problemId: configProblem.id }}
          resourceLabel={configProblem.title}
          open={configDialogOpen}
          onOpenChange={setConfigDialogOpen}
        />
      )}
    </>
  );
}
