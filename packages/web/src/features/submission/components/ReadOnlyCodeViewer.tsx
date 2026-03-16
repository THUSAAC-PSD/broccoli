import { useTranslation } from '@broccoli/web-sdk/i18n';
import { useTheme } from '@broccoli/web-sdk/theme';
import { Badge } from '@broccoli/web-sdk/ui';
import { cn } from '@broccoli/web-sdk/utils';
import Editor from '@monaco-editor/react';
import { ChevronDown, FileText } from 'lucide-react';
import { useState } from 'react';

const LANG_TO_MONACO: Record<string, string> = {
  cpp: 'cpp',
  c: 'c',
  python3: 'python',
  java: 'java',
  rust: 'rust',
  go: 'go',
  javascript: 'javascript',
  typescript: 'typescript',
};

const LANG_DISPLAY: Record<string, string> = {
  cpp: 'C++',
  c: 'C',
  python3: 'Python',
  java: 'Java',
  rust: 'Rust',
  go: 'Go',
  javascript: 'JS',
  typescript: 'TS',
};

interface ReadOnlyCodeViewerProps {
  files: { filename: string; content: string }[];
  language?: string;
  defaultOpen?: boolean;
}

export function ReadOnlyCodeViewer({
  files,
  language,
  defaultOpen = false,
}: ReadOnlyCodeViewerProps) {
  const [open, setOpen] = useState(defaultOpen);
  const [activeIndex, setActiveIndex] = useState(0);
  const { theme } = useTheme();
  const { t } = useTranslation();

  if (files.length === 0) return null;

  const activeFile = files[activeIndex] ?? files[0];
  const lineCount = activeFile.content.split('\n').length;
  const editorHeight = Math.min(Math.max(lineCount * 19, 80), 400);
  const monacoLang = language
    ? (LANG_TO_MONACO[language] ?? language)
    : 'plaintext';
  const langLabel = language ? (LANG_DISPLAY[language] ?? language) : null;

  return (
    <div className="overflow-hidden rounded-lg border border-border">
      {/* Header */}
      <button
        type="button"
        onClick={() => setOpen(!open)}
        className={cn(
          'flex w-full cursor-pointer items-center justify-between gap-2 border-none px-3 py-2',
          'bg-muted/50 hover:bg-muted transition-colors',
        )}
      >
        <div className="flex min-w-0 items-center gap-2">
          <FileText className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
          <span className="truncate font-mono text-xs text-foreground">
            {t('submissionDetail.sourceCode')}
          </span>
          {langLabel && (
            <Badge variant="outline" className="px-1.5 py-0 text-[10px]">
              {langLabel}
            </Badge>
          )}
        </div>
        <ChevronDown
          className={cn(
            'h-4 w-4 shrink-0 text-muted-foreground transition-transform duration-200',
            open && 'rotate-180',
          )}
        />
      </button>

      {/* Collapsible body — CSS grid-template-rows animation */}
      <div
        className={cn(
          'grid transition-[grid-template-rows] duration-250 ease-in-out',
          open ? 'grid-rows-[1fr]' : 'grid-rows-[0fr]',
        )}
      >
        <div className="overflow-hidden">
          {/* Multi-file tabs */}
          {files.length > 1 && (
            <div className="flex gap-0 border-b border-border bg-muted/30">
              {files.map((file, i) => (
                <button
                  key={file.filename}
                  type="button"
                  onClick={(e) => {
                    e.stopPropagation();
                    setActiveIndex(i);
                  }}
                  className={cn(
                    'border-b-2 px-3 py-1.5 font-mono text-xs transition-colors',
                    i === activeIndex
                      ? 'border-primary text-foreground bg-background'
                      : 'border-transparent text-muted-foreground hover:text-foreground hover:bg-muted/50',
                  )}
                >
                  {file.filename}
                </button>
              ))}
            </div>
          )}

          {/* Monaco editor */}
          <div style={{ height: editorHeight }}>
            {open && (
              <Editor
                height="100%"
                language={monacoLang}
                value={activeFile.content}
                theme={theme === 'dark' ? 'vs-dark' : 'vs'}
                options={{
                  readOnly: true,
                  domReadOnly: true,
                  minimap: { enabled: false },
                  fontSize: 13,
                  lineNumbers: 'on',
                  scrollBeyondLastLine: false,
                  renderLineHighlight: 'none',
                  overviewRulerLanes: 0,
                  hideCursorInOverviewRuler: true,
                  overviewRulerBorder: false,
                  scrollbar: { vertical: 'auto', horizontal: 'auto' },
                  contextmenu: false,
                  selectionHighlight: false,
                  occurrencesHighlight: 'off',
                  folding: false,
                  lineDecorationsWidth: 0,
                  padding: { top: 8, bottom: 8 },
                }}
              />
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
