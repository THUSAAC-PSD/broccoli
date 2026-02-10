import { Maximize2, Minimize2 } from 'lucide-react';

import { Button } from '@/components/ui/button';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Markdown } from '@/components/Markdown';

interface Example {
  input: string;
  output: string;
  explanation?: string;
}

interface ProblemDescriptionProps {
  description: string;
  inputFormat: string;
  outputFormat: string;
  examples: Example[];
  notes?: string;
  isFullscreen?: boolean;
  onToggleFullscreen?: () => void;
}

export function ProblemDescription({
  description,
  inputFormat,
  outputFormat,
  examples,
  notes,
  isFullscreen,
  onToggleFullscreen,
}: ProblemDescriptionProps) {
  return (
    <Card className="h-full overflow-y-auto">
      <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-4">
        <CardTitle>Problem</CardTitle>
        {onToggleFullscreen && (
          <Button
            variant="ghost"
            size="sm"
            onClick={onToggleFullscreen}
            aria-label="Toggle fullscreen"
          >
            {isFullscreen ? (
              <Minimize2 className="h-4 w-4" />
            ) : (
              <Maximize2 className="h-4 w-4" />
            )}
          </Button>
        )}
      </CardHeader>
      <CardContent className="space-y-6">
        <section>
          <h3 className="text-base font-bold mb-3">Description</h3>
          <Markdown>{description}</Markdown>
        </section>

        <section>
          <h3 className="text-base font-bold mb-3">Input</h3>
          <Markdown>{inputFormat}</Markdown>
        </section>

        <section>
          <h3 className="text-base font-bold mb-3">Output</h3>
          <Markdown>{outputFormat}</Markdown>
        </section>

        <section className="space-y-4">
          <h3 className="text-base font-bold">Examples</h3>
          {examples.map((example, index) => (
            <div key={index} className="border rounded-lg overflow-hidden">
              <div className="grid grid-cols-2 divide-x">
                <div>
                  <div className="bg-muted/50 px-4 py-2 font-medium text-sm border-b">
                    Input
                  </div>
                  <pre className="p-4 text-sm font-mono overflow-x-auto">
                    {example.input}
                  </pre>
                </div>
                <div>
                  <div className="bg-muted/50 px-4 py-2 font-medium text-sm border-b">
                    Output
                  </div>
                  <pre className="p-4 text-sm font-mono overflow-x-auto">
                    {example.output}
                  </pre>
                </div>
              </div>
              {example.explanation && (
                <div className="px-4 py-3 bg-muted/30 text-sm border-t">
                  <span className="font-medium">Explanation: </span>
                  {example.explanation}
                </div>
              )}
            </div>
          ))}
        </section>

        {notes && (
          <section>
            <h3 className="text-base font-bold mb-3">Notes</h3>
            <div className="p-4 bg-amber-50 dark:bg-amber-950/20 border border-amber-200 dark:border-amber-900 rounded-lg">
              <Markdown>{notes}</Markdown>
            </div>
          </section>
        )}
      </CardContent>
    </Card>
  );
}
