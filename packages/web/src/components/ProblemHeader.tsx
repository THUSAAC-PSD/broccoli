import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from '@/components/ui/card';

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
  io,
  timeLimit,
  memoryLimit,
}: ProblemHeaderProps) {
  return (
    <Card className="w-full shadow-sm border border-border bg-background">
      <CardHeader className="pb-3">
        <CardTitle className="text-2xl font-semibold tracking-tight">
          {id}. {title}
        </CardTitle>
        <CardDescription className="text-base text-muted-foreground">
          <span className="font-medium text-foreground">Type:</span> {type}
        </CardDescription>
      </CardHeader>

      <CardContent className="text-sm text-muted-foreground grid grid-cols-2 sm:grid-cols-3 gap-y-1">
        <div>
          <span className="font-medium text-foreground">File IO:</span> {io}
        </div>
        <div>
          <span className="font-medium text-foreground">Time Limit:</span>{' '}
          {timeLimit}
        </div>
        <div>
          <span className="font-medium text-foreground">Memory Limit:</span>{' '}
          {memoryLimit}
        </div>
      </CardContent>
    </Card>
  );
}
