import { useTranslation } from '@broccoli/web-sdk/i18n';
import { Skeleton } from '@broccoli/web-sdk/ui';

import { Markdown } from '@/components/Markdown';

import { ProblemSamplesSection } from './ProblemSamplesSection';

type SampleContentMap = Record<number, { input?: string; output?: string }>;

type CopySampleHandler = (
  tcId: number,
  sampleIndex: number,
  type: 'input' | 'output',
  anchorEl: HTMLElement,
  inlineContent?: string,
) => Promise<void>;

type DownloadSampleHandler = (
  tcId: number,
  sampleIndex: number,
  type: 'input' | 'output',
) => Promise<void>;

interface ProblemSample {
  id: number;
  input_size: number;
  output_size: number;
}

interface ProblemData {
  content: string;
  samples: ProblemSample[];
}

interface ProblemDescriptionTabProps {
  problem: ProblemData | undefined;
  isLoading: boolean;
  hasError: boolean;
  sampleContents: SampleContentMap;
  copiedKey: string | null;
  onCopySample: CopySampleHandler;
  onDownloadSample: DownloadSampleHandler;
}

export function ProblemDescriptionTab({
  problem,
  isLoading,
  hasError,
  sampleContents,
  copiedKey,
  onCopySample,
  onDownloadSample,
}: ProblemDescriptionTabProps) {
  const { t } = useTranslation();

  if (isLoading) {
    return (
      <div className="space-y-3">
        <Skeleton className="h-5 w-64" />
        <Skeleton className="h-5 w-48" />
        <Skeleton className="h-24 w-full" />
      </div>
    );
  }

  if (hasError) {
    return (
      <div className="text-sm text-destructive">{t('problem.loadError')}</div>
    );
  }

  if (!problem) return null;

  return (
    <div className="prose prose-sm dark:prose-invert max-w-none">
      <Markdown>{problem.content}</Markdown>

      <ProblemSamplesSection
        samples={problem.samples}
        sampleContents={sampleContents}
        copiedKey={copiedKey}
        onCopySample={onCopySample}
        onDownloadSample={onDownloadSample}
      />
    </div>
  );
}
