import { useTheme } from '@broccoli/web-sdk/theme';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@broccoli/web-sdk/ui';
import Editor from '@monaco-editor/react';
import { useState } from 'react';

import { Markdown } from '@/components/Markdown';

interface MarkdownEditorProps {
  id?: string;
  value: string;
  onChange: (value: string) => void;
  minHeight?: number;
  placeholder?: string;
}

export function MarkdownEditor({
  id,
  value,
  onChange,
  minHeight = 200,
  placeholder,
}: MarkdownEditorProps) {
  const { theme } = useTheme();
  const [tab, setTab] = useState<string>('write');

  return (
    <Tabs value={tab} onValueChange={setTab}>
      <TabsList className="h-8">
        <TabsTrigger value="write" className="text-xs px-3 py-1">
          Write
        </TabsTrigger>
        <TabsTrigger value="preview" className="text-xs px-3 py-1">
          Preview
        </TabsTrigger>
      </TabsList>
      <TabsContent value="write" className="mt-1">
        <div
          className="border rounded-md overflow-hidden"
          style={{ minHeight }}
        >
          <Editor
            height={`${minHeight}px`}
            language="markdown"
            value={value}
            onChange={(v) => onChange(v ?? '')}
            theme={theme === 'dark' ? 'vs-dark' : 'vs'}
            options={{
              minimap: { enabled: false },
              fontSize: 13,
              lineNumbers: 'off',
              scrollBeyondLastLine: false,
              automaticLayout: true,
              wordWrap: 'on',
              tabSize: 2,
              renderLineHighlight: 'none',
              overviewRulerLanes: 0,
              hideCursorInOverviewRuler: true,
              overviewRulerBorder: false,
              scrollbar: { vertical: 'auto', horizontal: 'hidden' },
              padding: { top: 8, bottom: 8 },
              placeholder,
            }}
          />
        </div>
      </TabsContent>
      <TabsContent value="preview" className="mt-1">
        <div
          className="border rounded-md p-4 overflow-y-auto"
          style={{ minHeight }}
        >
          {value.trim() ? (
            <Markdown>{value}</Markdown>
          ) : (
            <p className="text-sm text-muted-foreground italic">
              Nothing to preview
            </p>
          )}
        </div>
      </TabsContent>
      {id && <input type="hidden" id={id} />}
    </Tabs>
  );
}
