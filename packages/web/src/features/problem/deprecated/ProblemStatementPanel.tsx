import { useTranslation } from '@broccoli/web-sdk/i18n';
import { Button, Skeleton } from '@broccoli/web-sdk/ui';
import { formatBytes } from '@broccoli/web-sdk/utils';
import { Check, Copy } from 'lucide-react';

import { Markdown } from '@/components/Markdown';

import { useProblemDockContext } from './dock/ProblemDockContext';

export function ProblemStatementPanel() {
  const { t } = useTranslation();
  const {
    problem,
    isLoading,
    error,
    sampleContents,
    copiedKey,
    onCopySample,
    onDownloadSample,
  } = useProblemDockContext();

  if (isLoading) {
    return (
      <div className="p-6 space-y-3">
        <Skeleton className="h-5 w-64" />
        <Skeleton className="h-5 w-48" />
        <Skeleton className="h-24 w-full" />
      </div>
    );
  }

  if (error) {
    return (
      <div className="p-6 text-sm text-destructive">
        {t('problem.loadError')}
      </div>
    );
  }

  if (!problem) return null;

  return (
    <div className="h-full overflow-y-auto p-5">
      <div className="prose prose-sm dark:prose-invert max-w-none">
        <Markdown>{problem.content}</Markdown>

        {problem.samples.length > 0 && (
          <section className="mt-6 space-y-4">
            <h3 className="text-base font-bold">{t('problem.examples')}</h3>

            {problem.samples.map((sample, index) => {
              const sampleNumber = index + 1;
              const sampleContent = sampleContents[sample.id] ?? {};

              return (
                <div key={sample.id} className="space-y-3">
                  <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                    {/* Input */}
                    <div className="space-y-2">
                      <div className="flex items-center justify-between px-1 text-sm font-medium">
                        {`${t('problem.input')} #${sampleNumber}`}
                        <Button
                          type="button"
                          variant="ghost"
                          size="icon"
                          className="h-7 w-7"
                          title={t('problem.copy')}
                          onClick={(event) =>
                            onCopySample(
                              sample.id,
                              sampleNumber,
                              'input',
                              event.currentTarget,
                              sampleContent.input,
                            )
                          }
                        >
                          {copiedKey === `input-${sample.id}` ? (
                            <Check className="h-4 w-4" />
                          ) : (
                            <Copy className="h-4 w-4" />
                          )}
                        </Button>
                      </div>
                      <div className="border rounded-lg overflow-hidden">
                        {sampleContent.input !== undefined ? (
                          <pre className="p-4 text-sm font-mono overflow-x-auto mb-0">
                            {sampleContent.input}
                          </pre>
                        ) : (
                          <div className="p-4 text-sm">
                            <button
                              type="button"
                              className="text-primary underline underline-offset-2 decoration-primary/40 hover:decoration-primary/80 transition-colors"
                              onClick={() =>
                                onDownloadSample(
                                  sample.id,
                                  sampleNumber,
                                  'input',
                                )
                              }
                            >
                              {t('problem.downloadSampleFile', {
                                file: `sample${sampleNumber}.in`,
                                size: formatBytes(sample.input_size),
                              })}
                            </button>
                          </div>
                        )}
                      </div>
                    </div>

                    {/* Output */}
                    <div className="space-y-2">
                      <div className="flex items-center justify-between px-1 text-sm font-medium">
                        {`${t('problem.output')} #${sampleNumber}`}
                        <Button
                          type="button"
                          variant="ghost"
                          size="icon"
                          className="h-7 w-7"
                          title={t('problem.copy')}
                          onClick={(event) =>
                            onCopySample(
                              sample.id,
                              sampleNumber,
                              'output',
                              event.currentTarget,
                              sampleContent.output,
                            )
                          }
                        >
                          {copiedKey === `output-${sample.id}` ? (
                            <Check className="h-4 w-4" />
                          ) : (
                            <Copy className="h-4 w-4" />
                          )}
                        </Button>
                      </div>
                      <div className="border rounded-lg overflow-hidden">
                        {sampleContent.output !== undefined ? (
                          <pre className="p-4 text-sm font-mono overflow-x-auto mb-0">
                            {sampleContent.output}
                          </pre>
                        ) : (
                          <div className="p-4 text-sm">
                            <button
                              type="button"
                              className="text-primary underline underline-offset-2 decoration-primary/40 hover:decoration-primary/80 transition-colors"
                              onClick={() =>
                                onDownloadSample(
                                  sample.id,
                                  sampleNumber,
                                  'output',
                                )
                              }
                            >
                              {t('problem.downloadSampleFile', {
                                file: `sample${sampleNumber}.out`,
                                size: formatBytes(sample.output_size),
                              })}
                            </button>
                          </div>
                        )}
                      </div>
                    </div>
                  </div>
                </div>
              );
            })}
          </section>
        )}
      </div>
    </div>
  );
}
