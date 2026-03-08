import { useTranslation } from '@broccoli/web-sdk/i18n';
import { Slot } from '@broccoli/web-sdk/react';
import { useTheme } from '@broccoli/web-sdk/theme';
import Editor from '@monaco-editor/react';
import JSZip from 'jszip';
import {
  ChevronDown,
  Maximize2,
  Minimize2,
  Play,
  Plus,
  Upload,
  X,
} from 'lucide-react';
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

export interface EditorFile {
  id: string;
  filename: string;
  content: string;
}

interface CodeEditorProps {
  onSubmit?: (files: EditorFile[], language: string) => void;
  onRun?: (files: EditorFile[], language: string) => void;
  isFullscreen?: boolean;
  onToggleFullscreen?: () => void;
  /** Unique key for persisting code to localStorage (e.g. problem ID). */
  storageKey?: string;
}

const EXT_TO_MONACO: Record<string, string> = {
  cpp: 'cpp',
  cc: 'cpp',
  cxx: 'cpp',
  'c++': 'cpp',
  hpp: 'cpp',
  h: 'c',
  c: 'c',
  py: 'python',
  java: 'java',
  js: 'javascript',
  ts: 'typescript',
  json: 'json',
  xml: 'xml',
  txt: 'plaintext',
  md: 'markdown',
  sh: 'shell',
  yml: 'yaml',
  yaml: 'yaml',
};

const EXT_TO_LANGUAGE_ID: Record<string, string> = {
  cpp: 'cpp',
  cc: 'cpp',
  cxx: 'cpp',
  'c++': 'cpp',
  hpp: 'cpp',
  c: 'c',
  h: 'c',
  py: 'python',
  java: 'java',
};

function getMonacoLanguage(filename: string): string {
  const ext = filename.split('.').pop()?.toLowerCase() ?? '';
  return EXT_TO_MONACO[ext] ?? 'plaintext';
}

function detectLanguageFromFiles(files: EditorFile[]): string | null {
  for (const file of files) {
    const ext = file.filename.split('.').pop()?.toLowerCase() ?? '';
    const langId = EXT_TO_LANGUAGE_ID[ext];
    if (langId) return langId;
  }
  return null;
}

function getStorageKeys(storageKey: string) {
  return {
    code: `broccoli-editor-${storageKey}-code`,
    lang: `broccoli-editor-${storageKey}-lang`,
  };
}

let fileIdCounter = 0;
function nextFileId() {
  return `file-${++fileIdCounter}-${Date.now()}`;
}

const FILENAME_MAP: Record<string, string> = {
  cpp: 'solution.cpp',
  c: 'solution.c',
  python: 'solution.py',
  java: 'Main.java',
};

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

  // Multi-file tabs state
  const [files, setFiles] = useState<EditorFile[]>(() => {
    const defaultFilename = FILENAME_MAP[selectedLanguage.id] ?? 'solution.txt';
    let initialContent = selectedLanguage.template;
    if (storageKey) {
      const saved = localStorage.getItem(getStorageKeys(storageKey).code);
      if (saved != null) initialContent = saved;
    }
    return [
      { id: nextFileId(), filename: defaultFilename, content: initialContent },
    ];
  });

  const [activeFileId, setActiveFileId] = useState<string>(files[0].id);

  const activeFile = files.find((f) => f.id === activeFileId) ?? files[0];

  const editorRef = useRef<editor.IStandaloneCodeEditor | null>(null);
  const fileInputRef = useRef<HTMLInputElement | null>(null);
  const { theme } = useTheme();

  // Reset editor state when storageKey changes (switching problems)
  const prevStorageKey = useRef(storageKey);
  useEffect(() => {
    if (storageKey === prevStorageKey.current) return;
    prevStorageKey.current = storageKey;

    // Load saved language
    let lang = LANGUAGES[0];
    if (storageKey) {
      const savedLang = localStorage.getItem(getStorageKeys(storageKey).lang);
      if (savedLang) {
        const found = LANGUAGES.find((l) => l.id === savedLang);
        if (found) lang = found;
      }
    }
    setSelectedLanguage(lang);

    // Load saved code
    const defaultFilename = FILENAME_MAP[lang.id] ?? 'solution.txt';
    let content = lang.template;
    if (storageKey) {
      const savedCode = localStorage.getItem(getStorageKeys(storageKey).code);
      if (savedCode != null) content = savedCode;
    }
    const newFile = { id: nextFileId(), filename: defaultFilename, content };
    setFiles([newFile]);
    setActiveFileId(newFile.id);
  }, [storageKey]);

  // Auto-save primary file to localStorage
  useEffect(() => {
    if (!storageKey) return;
    const keys = getStorageKeys(storageKey);
    // Save the first file's content as the "code" for backward compat
    if (files.length > 0) {
      localStorage.setItem(keys.code, files[0].content);
    }
    localStorage.setItem(keys.lang, selectedLanguage.id);
  }, [storageKey, files, selectedLanguage]);

  const handleLanguageChange = useCallback(
    (language: Language) => {
      setSelectedLanguage(language);
      const newFilename = FILENAME_MAP[language.id] ?? 'solution.txt';
      // If there's only 1 file (the default), reset it
      if (files.length === 1 && files[0].filename.startsWith('solution')) {
        setFiles([
          {
            id: files[0].id,
            filename: newFilename,
            content: language.template,
          },
        ]);
      }
    },
    [files],
  );

  const updateFileContent = useCallback((fileId: string, content: string) => {
    setFiles((prev) =>
      prev.map((f) => (f.id === fileId ? { ...f, content } : f)),
    );
  }, []);

  const closeFile = useCallback(
    (fileId: string) => {
      setFiles((prev) => {
        if (prev.length <= 1) return prev; // Don't close last file
        const next = prev.filter((f) => f.id !== fileId);
        if (activeFileId === fileId) {
          setActiveFileId(next[0].id);
        }
        return next;
      });
    },
    [activeFileId],
  );

  const addFiles = useCallback(
    (newFiles: EditorFile[]) => {
      if (newFiles.length === 0) return;

      // Detect language from uploaded files
      const detectedLang = detectLanguageFromFiles(newFiles);
      if (detectedLang) {
        const lang = LANGUAGES.find((l) => l.id === detectedLang);
        if (lang) setSelectedLanguage(lang);
      }

      setFiles((prev) => {
        // If there's only the default template file, replace it
        const isDefault =
          prev.length === 1 &&
          prev[0].content === selectedLanguage.template &&
          prev[0].filename.startsWith('solution');

        const base = isDefault ? [] : prev;
        return [...base, ...newFiles];
      });

      // Activate the first new file
      setActiveFileId(newFiles[0].id);
    },
    [selectedLanguage],
  );

  const processUploadedFiles = useCallback(
    async (fileList: FileList) => {
      const newFiles: EditorFile[] = [];

      for (const file of Array.from(fileList)) {
        if (
          file.name.endsWith('.zip') ||
          file.type === 'application/zip' ||
          file.type === 'application/x-zip-compressed'
        ) {
          // Process zip file
          try {
            const arrayBuffer = await file.arrayBuffer();
            const zip = await JSZip.loadAsync(arrayBuffer);

            const entries: { path: string; file: JSZip.JSZipObject }[] = [];
            zip.forEach((relativePath, zipEntry) => {
              if (!zipEntry.dir) {
                entries.push({ path: relativePath, file: zipEntry });
              }
            });

            for (const entry of entries) {
              // Skip hidden files / OS metadata
              const name = entry.path.split('/').pop() ?? entry.path;
              if (name.startsWith('.') || name.startsWith('__MACOSX')) continue;
              if (entry.path.includes('__MACOSX/')) continue;

              try {
                const content = await entry.file.async('string');
                newFiles.push({
                  id: nextFileId(),
                  filename: entry.path,
                  content,
                });
              } catch {
                // Skip binary files that can't be read as text
              }
            }
          } catch (err) {
            console.error('Failed to process zip file:', err);
          }
        } else {
          // Process regular file
          try {
            const content = await file.text();
            newFiles.push({
              id: nextFileId(),
              filename: file.name,
              content,
            });
          } catch (err) {
            console.error('Failed to read file:', err);
          }
        }
      }

      addFiles(newFiles);
    },
    [addFiles],
  );

  const addNewFile = useCallback(() => {
    const ext = FILENAME_MAP[selectedLanguage.id]?.split('.').pop() ?? 'txt';
    // Find a unique name like "file1.cpp", "file2.cpp", etc.
    let index = 1;
    const existingNames = new Set(files.map((f) => f.filename));
    while (existingNames.has(`file${index}.${ext}`)) index++;
    const filename = `file${index}.${ext}`;
    const newFile: EditorFile = { id: nextFileId(), filename, content: '' };
    setFiles((prev) => [...prev, newFile]);
    setActiveFileId(newFile.id);
  }, [selectedLanguage, files]);

  const handleUploadClick = () => {
    fileInputRef.current?.click();
  };

  const handleFileInputChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    if (e.target.files && e.target.files.length > 0) {
      processUploadedFiles(e.target.files);
      // Reset so the same file can be selected again
      e.target.value = '';
    }
  };

  const handleSubmit = () => {
    if (onSubmit) {
      onSubmit(files, selectedLanguage.id);
    }
  };

  const handleRun = () => {
    if (onRun) {
      onRun(files, selectedLanguage.id);
    }
  };

  const handleEditorDidMount = (ed: editor.IStandaloneCodeEditor) => {
    editorRef.current = ed;
  };

  const handleDrop = useCallback(
    (e: React.DragEvent) => {
      e.preventDefault();
      e.stopPropagation();
      if (e.dataTransfer.files.length > 0) {
        processUploadedFiles(e.dataTransfer.files);
      }
    },
    [processUploadedFiles],
  );

  const handleDragOver = useCallback((e: React.DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
  }, []);

  return (
    <Card
      className="h-full flex flex-col"
      onDrop={handleDrop}
      onDragOver={handleDragOver}
    >
      <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-4">
        <CardTitle>{t('editor.title')}</CardTitle>
        <div className="flex items-center gap-2">
          <Slot
            name="problem-detail.editor.toolbar"
            as="div"
            className="flex items-center gap-2"
          />
          <Button
            variant="ghost"
            size="sm"
            onClick={handleUploadClick}
            title={t('editor.upload')}
          >
            <Upload className="h-4 w-4" />
          </Button>
          <input
            ref={fileInputRef}
            type="file"
            multiple
            accept=".c,.cpp,.cc,.cxx,.h,.hpp,.py,.java,.js,.ts,.txt,.json,.xml,.md,.sh,.yml,.yaml,.zip"
            className="hidden"
            onChange={handleFileInputChange}
          />
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
        {/* File tabs */}
        <div className="flex items-center gap-0 overflow-x-auto border-b -mt-2">
          {files.map((file) => (
            <button
              key={file.id}
              type="button"
              onClick={() => setActiveFileId(file.id)}
              className={`group flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium border-b-2 transition-colors whitespace-nowrap ${
                file.id === activeFileId
                  ? 'border-primary text-foreground'
                  : 'border-transparent text-muted-foreground hover:text-foreground hover:border-muted-foreground/30'
              }`}
            >
              <span>{file.filename.split('/').pop()}</span>
              {files.length > 1 && (
                <span
                  role="button"
                  tabIndex={0}
                  onClick={(e) => {
                    e.stopPropagation();
                    closeFile(file.id);
                  }}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter' || e.key === ' ') {
                      e.stopPropagation();
                      closeFile(file.id);
                    }
                  }}
                  className="opacity-0 group-hover:opacity-100 hover:text-destructive transition-opacity"
                >
                  <X className="h-3 w-3" />
                </span>
              )}
            </button>
          ))}
          <button
            type="button"
            onClick={addNewFile}
            title={t('editor.addFile')}
            className="flex items-center px-2 py-1.5 text-muted-foreground hover:text-foreground transition-colors border-b-2 border-transparent"
          >
            <Plus className="h-3.5 w-3.5" />
          </button>
        </div>

        <div className="flex-1 min-h-[400px] border rounded-lg overflow-hidden">
          <Editor
            height="100%"
            language={getMonacoLanguage(activeFile.filename)}
            value={activeFile.content}
            onChange={(value) => updateFileContent(activeFile.id, value || '')}
            onMount={handleEditorDidMount}
            theme={theme === 'dark' ? 'vs-dark' : 'vs'}
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
