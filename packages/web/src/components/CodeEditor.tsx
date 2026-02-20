import { useTranslation } from '@broccoli/sdk/i18n';
import { Slot } from '@broccoli/sdk/react';
import Editor from '@monaco-editor/react';
import { ChevronDown, Maximize2, Minimize2, Play } from 'lucide-react';
import type { editor } from 'monaco-editor';
import { useCallback, useEffect, useRef, useState } from 'react';

import { Button } from '@/components/ui/button';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { useTheme } from '@/hooks/use-theme';

interface Language {
  id: string;
  name: string;
  monacoLanguage: string;
  template: string;
}

const LANGUAGES: Language[] = [
  {
    id: 'cpp',
    name: 'C++',
    monacoLanguage: 'cpp',
    template: `#include <iostream>
using namespace std;

int main() {
    // Your code here
    return 0;
}`,
  },
  {
    id: 'python',
    name: 'Python',
    monacoLanguage: 'python',
    template: `# Your code here
`,
  },
  {
    id: 'java',
    name: 'Java',
    monacoLanguage: 'java',
    template: `public class Main {
    public static void main(String[] args) {
        // Your code here
    }
}`,
  },
  {
    id: 'c',
    name: 'C',
    monacoLanguage: 'c',
    template: `#include <stdio.h>

int main() {
    // Your code here
    return 0;
}`,
  },
];

interface CodeEditorProps {
  onSubmit?: (code: string, language: string) => void;
  onRun?: (code: string, language: string) => void;
  isFullscreen?: boolean;
  onToggleFullscreen?: () => void;
  /** Unique key for persisting code to localStorage (e.g. problem ID). */
  storageKey?: string;
}

function getStorageKeys(storageKey: string) {
  return {
    code: `broccoli-editor-${storageKey}-code`,
    lang: `broccoli-editor-${storageKey}-lang`,
  };
}

export function CodeEditor({
  onSubmit,
  onRun,
  isFullscreen,
  onToggleFullscreen,
  storageKey,
}: CodeEditorProps) {
  const { t } = useTranslation();

  const [selectedLanguage, setSelectedLanguage] = useState<Language>(() => {
    if (storageKey) {
      const saved = localStorage.getItem(getStorageKeys(storageKey).lang);
      if (saved) {
        const found = LANGUAGES.find((l) => l.id === saved);
        if (found) return found;
      }
    }
    return LANGUAGES[0];
  });

  const [code, setCode] = useState(() => {
    if (storageKey) {
      const saved = localStorage.getItem(getStorageKeys(storageKey).code);
      if (saved != null) return saved;
    }
    return selectedLanguage.template;
  });

  const editorRef = useRef<editor.IStandaloneCodeEditor | null>(null);
  const { theme } = useTheme();

  // Auto-save to localStorage
  useEffect(() => {
    if (!storageKey) return;
    const keys = getStorageKeys(storageKey);
    localStorage.setItem(keys.code, code);
    localStorage.setItem(keys.lang, selectedLanguage.id);
  }, [storageKey, code, selectedLanguage]);

  const handleLanguageChange = useCallback((language: Language) => {
    setSelectedLanguage(language);
    setCode(language.template);
  }, []);

  const handleSubmit = () => {
    if (onSubmit) {
      onSubmit(code, selectedLanguage.id);
    }
  };

  const handleRun = () => {
    if (onRun) {
      onRun(code, selectedLanguage.id);
    }
  };

  const handleEditorDidMount = (editor: editor.IStandaloneCodeEditor) => {
    editorRef.current = editor;
  };

  return (
    <Card className="h-full flex flex-col">
      <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-4">
        <CardTitle>{t('editor.title')}</CardTitle>
        <div className="flex items-center gap-2">
          <Slot name="problem-detail.editor.toolbar" as="div" className="flex items-center gap-2" />
          {onToggleFullscreen && (
            <Button
              variant="ghost"
              size="sm"
              onClick={onToggleFullscreen}
              aria-label={t('editor.toggleFullscreen')}
            >
              {isFullscreen ? (
                <Minimize2 className="h-4 w-4" />
              ) : (
                <Maximize2 className="h-4 w-4" />
              )}
            </Button>
          )}
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button variant="outline" size="sm">
                {selectedLanguage.name}
                <ChevronDown className="ml-2 h-4 w-4" />
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end">
              {LANGUAGES.map((lang) => (
                <DropdownMenuItem
                  key={lang.id}
                  onClick={() => handleLanguageChange(lang)}
                >
                  {lang.name}
                </DropdownMenuItem>
              ))}
            </DropdownMenuContent>
          </DropdownMenu>
        </div>
      </CardHeader>
      <CardContent className="flex-1 flex flex-col gap-4 p-0 px-6 pb-6">
        <div className="flex-1 min-h-[400px] border rounded-lg overflow-hidden">
          <Editor
            height="100%"
            language={selectedLanguage.monacoLanguage}
            value={code}
            onChange={(value) => setCode(value || '')}
            onMount={handleEditorDidMount}
            theme={theme === 'dark' ? 'vs-dark' : 'light'}
            options={{
              minimap: { enabled: false },
              fontSize: 14,
              lineNumbers: 'on',
              roundedSelection: false,
              scrollBeyondLastLine: false,
              automaticLayout: true,
              tabSize: 4,
              wordWrap: 'on',
            }}
          />
        </div>
        <div className="flex gap-2 justify-end">
          <Button variant="outline" onClick={handleRun}>
            <Play className="mr-2 h-4 w-4" />
            {t('editor.run')}
          </Button>
          <Button onClick={handleSubmit}>{t('editor.submit')}</Button>
        </div>
      </CardContent>
    </Card>
  );
}
