import { useState, useRef } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { ChevronDown, Play, Maximize2, Minimize2 } from 'lucide-react';
import Editor from '@monaco-editor/react';
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
}

export function CodeEditor({ onSubmit, onRun, isFullscreen, onToggleFullscreen }: CodeEditorProps) {
  const [selectedLanguage, setSelectedLanguage] = useState<Language>(LANGUAGES[0]);
  const [code, setCode] = useState(selectedLanguage.template);
  const editorRef = useRef<any>(null);
  const { theme } = useTheme();

  const handleLanguageChange = (language: Language) => {
    setSelectedLanguage(language);
    setCode(language.template);
  };

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

  const handleEditorDidMount = (editor: any) => {
    editorRef.current = editor;
  };

  return (
    <Card className="h-full flex flex-col">
      <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-4">
        <CardTitle>Code</CardTitle>
        <div className="flex items-center gap-2">
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
        <div className="flex-1 border rounded-lg overflow-hidden">
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
            Run
          </Button>
          <Button onClick={handleSubmit}>Submit</Button>
        </div>
      </CardContent>
    </Card>
  );
}
