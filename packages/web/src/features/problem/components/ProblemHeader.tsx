import { Card, CardContent, CardHeader, CardTitle } from '@broccoli/web-sdk/ui';
import { Clock, Cpu, FileText } from 'lucide-react';

interface ProblemHeaderProps {
  id: string;
  title: string;
  type: string;
  io: string;
  timeLimit: string;
  memoryLimit: string;
}

export function ProblemHeader({
  id,
  title,
  type,
  timeLimit,
  memoryLimit,
}: ProblemHeaderProps) {
  return (
    <Card className="w-full shadow-none border-0 bg-transparent pt-3">
      <CardHeader className="px-0 pt-0 pb-4">
        <CardTitle className="text-3xl font-bold tracking-tight">
          {id}. {title}
        </CardTitle>
      </CardHeader>

      <CardContent className="flex flex-wrap gap-2 pt-0 px-0 pb-2">
        <span className="inline-flex items-center gap-1.5 rounded-full px-3 py-1 text-xs font-medium bg-pink-500/10 text-pink-600 dark:text-pink-400 ring-1 ring-pink-500/25">
          <FileText className="size-3" />
          {type}
        </span>
        <span className="inline-flex items-center gap-1.5 rounded-full px-3 py-1 text-xs font-medium bg-amber-500/10 text-amber-600 dark:text-amber-400 ring-1 ring-amber-500/25">
          <Clock className="size-3" />
          {timeLimit}
        </span>
        <span className="inline-flex items-center gap-1.5 rounded-full px-3 py-1 text-xs font-medium bg-blue-500/10 text-blue-600 dark:text-blue-400 ring-1 ring-blue-500/25">
          <Cpu className="size-3" />
          {memoryLimit}
        </span>
      </CardContent>
    </Card>
  );
}
