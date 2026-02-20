import { useEffect, useState } from 'react';

import { useApiClient } from '@broccoli/sdk/api';
import { useTranslation } from '@broccoli/sdk/i18n';
import { useQueryClient } from '@tanstack/react-query';

import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Separator } from '@/components/ui/separator';
import { Textarea } from '@/components/ui/textarea';

import { SwitchField } from './helpers';

interface TestCaseFormDialogProps {
  problemId: number;
  testCaseId?: number;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  testCasesQueryKey: (string | number)[];
}

export function TestCaseFormDialog({
  problemId,
  testCaseId,
  open,
  onOpenChange,
  testCasesQueryKey,
}: TestCaseFormDialogProps) {
  const { t } = useTranslation();
  const apiClient = useApiClient();
  const queryClient = useQueryClient();
  const isEdit = !!testCaseId;

  const [input, setInput] = useState('');
  const [expectedOutput, setExpectedOutput] = useState('');
  const [score, setScore] = useState(0);
  const [isSample, setIsSample] = useState(false);
  const [description, setDescription] = useState('');
  const [loading, setLoading] = useState(false);
  const [loadingData, setLoadingData] = useState(false);
  const [message, setMessage] = useState<{
    type: 'success' | 'error';
    text: string;
  } | null>(null);

  useEffect(() => {
    if (!open) return;
    setMessage(null);
    if (testCaseId) {
      setLoadingData(true);
      apiClient
        .GET('/problems/{id}/test-cases/{tc_id}', {
          params: { path: { id: problemId, tc_id: testCaseId } },
        })
        .then(({ data, error }) => {
          setLoadingData(false);
          if (error || !data) return;
          setInput(data.input);
          setExpectedOutput(data.expected_output);
          setScore(data.score);
          setIsSample(data.is_sample);
          setDescription(data.description ?? '');
        });
    } else {
      setInput('');
      setExpectedOutput('');
      setScore(0);
      setIsSample(false);
      setDescription('');
    }
  }, [apiClient, open, testCaseId, problemId]);

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    setLoading(true);
    setMessage(null);

    const body = {
      input,
      expected_output: expectedOutput,
      score,
      is_sample: isSample,
      description: description || null,
    };

    const result = isEdit
      ? await apiClient.PATCH('/problems/{id}/test-cases/{tc_id}', {
          params: { path: { id: problemId, tc_id: testCaseId! } },
          body,
        })
      : await apiClient.POST('/problems/{id}/test-cases', {
          params: { path: { id: problemId } },
          body,
        });

    setLoading(false);
    if (result.error) {
      setMessage({
        type: 'error',
        text: isEdit ? t('admin.editError') : t('admin.createError'),
      });
    } else {
      queryClient.invalidateQueries({ queryKey: testCasesQueryKey });
      onOpenChange(false);
    }
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-2xl max-h-[90vh] overflow-y-auto">
        <DialogHeader>
          <DialogTitle>
            {isEdit ? t('admin.testCases.edit') : t('admin.testCases.create')}
          </DialogTitle>
          <DialogDescription>
            {isEdit ? '' : t('admin.testCases.createDesc')}
          </DialogDescription>
        </DialogHeader>

        {loadingData ? (
          <div className="py-8 text-center text-muted-foreground">
            {t('admin.loading')}
          </div>
        ) : (
          <form onSubmit={handleSubmit} className="space-y-4">
            <div className="space-y-2">
              <Label htmlFor="tc-input">
                {t('admin.testCases.field.input')}
              </Label>
              <Textarea
                id="tc-input"
                value={input}
                onChange={(e) => setInput(e.target.value)}
                rows={6}
                className="font-mono text-sm"
                placeholder="4&#10;2 7 11 15&#10;9"
              />
            </div>

            <div className="space-y-2">
              <Label htmlFor="tc-expected-output">
                {t('admin.testCases.field.expectedOutput')}
              </Label>
              <Textarea
                id="tc-expected-output"
                value={expectedOutput}
                onChange={(e) => setExpectedOutput(e.target.value)}
                rows={6}
                className="font-mono text-sm"
                placeholder="0 1"
              />
            </div>

            <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
              <div className="space-y-2">
                <Label htmlFor="tc-score">
                  {t('admin.testCases.field.score')}
                </Label>
                <Input
                  id="tc-score"
                  type="number"
                  min={0}
                  max={10000}
                  value={score}
                  onChange={(e) => setScore(Number(e.target.value))}
                  required
                />
              </div>
              <div className="space-y-2">
                <Label htmlFor="tc-description">
                  {t('admin.testCases.field.description')}
                </Label>
                <Input
                  id="tc-description"
                  value={description}
                  onChange={(e) => setDescription(e.target.value)}
                  maxLength={256}
                  placeholder="Basic case"
                />
              </div>
            </div>

            <Separator />

            <div className="space-y-3">
              <Label className="text-sm text-muted-foreground">
                {t('admin.field.options')}
              </Label>
              <SwitchField
                id="tc-is-sample"
                label={t('admin.testCases.field.isSample')}
                checked={isSample}
                onCheckedChange={setIsSample}
              />
            </div>

            {message && (
              <div
                className={`rounded-md px-4 py-3 text-sm ${
                  message.type === 'success'
                    ? 'bg-green-500/10 text-green-500 border border-green-500/20'
                    : 'bg-destructive/10 text-destructive border border-destructive/20'
                }`}
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
                    : t('admin.testCases.create')}
              </Button>
            </DialogFooter>
          </form>
        )}
      </DialogContent>
    </Dialog>
  );
}
