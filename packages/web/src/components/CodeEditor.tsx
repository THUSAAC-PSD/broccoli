import { useApiClient } from '@broccoli/web-sdk/api';
import { useTranslation } from '@broccoli/web-sdk/i18n';
import { Slot } from '@broccoli/web-sdk/slot';
import { useTheme } from '@broccoli/web-sdk/theme';
import {
  Button,
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@broccoli/web-sdk/ui';
import type { Monaco } from '@monaco-editor/react';
import Editor from '@monaco-editor/react';
import { useQuery } from '@tanstack/react-query';
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
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import { KeybindingModeDropdown } from '@/components/KeybindingModeDropdown';
import {
  fetchSupportedLanguages,
  type SupportedLanguage,
} from '@/features/problem/api/fetch-supported-languages';
import { useEditorKeybindings } from '@/lib/use-editor-keybindings';

type Language = SupportedLanguage;

const FALLBACK_LANGUAGE: Language = {
  id: 'plaintext',
  name: 'Plain Text',
  defaultFilename: 'solution.txt',
  template: '',
};

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
  /** Currently selected contest type. */
  contestType?: string;
  /** Callback when contest type changes. */
  onContestTypeChange?: (contestType: string) => void;
  /** Available contest types from registry. */
  contestTypes?: string[];
  /**
   * Server-provided file names per language (from problem.submission_format).
   * Keys are language ids (e.g. "cpp", "java"), values are arrays of filenames.
   * Takes precedence over the built-in FILENAME_MAP defaults.
   */
  submissionFormat?: Record<string, string[]> | null;
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
  py: 'python3',
  java: 'java',
  js: 'javascript',
  mjs: 'javascript',
  cjs: 'javascript',
  ts: 'typescript',
  rs: 'rust',
  go: 'go',
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
    selectedLanguage: `broccoli-editor-${storageKey}-selected-language`,
  };
}

function getLanguageStorageKeys(storageKey: string, languageId: string) {
  const base = `broccoli-editor-${storageKey}-language-${languageId}`;
  return {
    files: `${base}-files`,
    activeFile: `${base}-active-file`,
    fileContent: (filename: string) =>
      `${base}-file-${encodeURIComponent(filename)}`,
  };
}

type PersistedEditorFile = {
  filename: string;
  content: string;
};

function parsePersistedFiles(raw: string | null): PersistedEditorFile[] {
  if (!raw) return [];
  try {
    const parsed = JSON.parse(raw) as unknown;
    if (!Array.isArray(parsed)) return [];
    return parsed
      .filter(
        (item): item is PersistedEditorFile =>
          typeof item === 'object' &&
          item !== null &&
          typeof (item as PersistedEditorFile).filename === 'string' &&
          typeof (item as PersistedEditorFile).content === 'string',
      )
      .map((item) => ({
        filename: item.filename,
        content: item.content,
      }));
  } catch {
    return [];
  }
}

function loadLanguageFiles(
  storageKey: string,
  languageId: string,
): PersistedEditorFile[] {
  const keys = getLanguageStorageKeys(storageKey, languageId);
  const filenames = parsePersistedFiles(localStorage.getItem(keys.files)).map(
    (file) => file.filename,
  );

  return filenames.map((filename) => ({
    filename,
    content: localStorage.getItem(keys.fileContent(filename)) ?? '',
  }));
}

function saveLanguageFiles(
  storageKey: string,
  languageId: string,
  files: EditorFile[],
) {
  const keys = getLanguageStorageKeys(storageKey, languageId);
  const persisted = files.map((file) => ({
    filename: file.filename,
    content: '',
  }));
  localStorage.setItem(keys.files, JSON.stringify(persisted));
  for (const file of files) {
    localStorage.setItem(keys.fileContent(file.filename), file.content);
  }
}

let fileIdCounter = 0;
function nextFileId() {
  return `file-${++fileIdCounter}-${Date.now()}`;
}

const FILENAME_MAP: Record<string, string> = {
  cpp: 'solution.cpp',
  c: 'solution.c',
  python3: 'solution.py',
  java: 'Main.java',
  javascript: 'solution.js',
  typescript: 'solution.ts',
  rust: 'solution.rs',
  go: 'solution.go',
};

/**
 * Returns the default filename for a language, preferring server-provided
 * submission_format over the built-in FILENAME_MAP.
 */
function getDefaultFilename(
  languageId: string,
  submissionFormat?: Record<string, string[]> | null,
): string {
  if (submissionFormat) {
    const serverFiles = submissionFormat[languageId];
    if (serverFiles && serverFiles.length > 0) {
      return serverFiles[0];
    }
  }
  return FILENAME_MAP[languageId] ?? 'solution.txt';
}

function getConfiguredFilenames(
  languageId: string,
  submissionFormat?: Record<string, string[]> | null,
): string[] {
  const names = submissionFormat?.[languageId] ?? [];
  return names
    .map((name) => name.trim())
    .filter(
      (name, index, arr) => name.length > 0 && arr.indexOf(name) === index,
    );
}

export function CodeEditor({
  onSubmit,
  onRun,
  isFullscreen,
  onToggleFullscreen,
  storageKey,
  contestType,
  onContestTypeChange,
  contestTypes,
  submissionFormat,
}: CodeEditorProps) {
  const { t } = useTranslation();
  const apiClient = useApiClient();
  const { data: supportedLanguages = [] } = useQuery({
    queryKey: ['supported-languages'],
    queryFn: () => fetchSupportedLanguages(apiClient),
    staleTime: 5 * 60 * 1000,
  });

  const availableLanguages = useMemo(() => {
    if (supportedLanguages.length === 0) return [FALLBACK_LANGUAGE];
    if (!submissionFormat) return supportedLanguages;
    const configured = supportedLanguages.filter((lang) => {
      const files = getConfiguredFilenames(lang.id, submissionFormat);
      return files.length > 0;
    });
    return configured.length > 0 ? configured : supportedLanguages;
  }, [submissionFormat, supportedLanguages]);

  const isSubmissionFormatLocked = useMemo(
    () =>
      Object.keys(submissionFormat ?? {}).some(
        (languageId) =>
          getConfiguredFilenames(languageId, submissionFormat).length > 0,
      ),
    [submissionFormat],
  );

  const [selectedLanguage, setSelectedLanguage] = useState<Language>(() => {
    if (storageKey) {
      const saved = localStorage.getItem(
        getStorageKeys(storageKey).selectedLanguage,
      );
      if (saved) {
        const found = availableLanguages.find((l) => l.id === saved);
        if (found) return found;
      }
    }
    return availableLanguages[0];
  });

  const buildFilesForLanguage = useCallback(
    (language: Language, previousFiles: EditorFile[]): EditorFile[] => {
      const configuredNames = getConfiguredFilenames(
        language.id,
        submissionFormat,
      );
      if (configuredNames.length === 0) {
        const fallbackFilename = getDefaultFilename(
          language.id,
          submissionFormat,
        );
        const keep = previousFiles.find((f) => f.filename === fallbackFilename);
        return [
          {
            id: keep?.id ?? nextFileId(),
            filename: fallbackFilename,
            content: keep?.content ?? language.template,
          },
        ];
      }

      return configuredNames.map((filename, index) => {
        const keep = previousFiles.find((f) => f.filename === filename);
        return {
          id: keep?.id ?? nextFileId(),
          filename,
          content: keep?.content ?? (index === 0 ? language.template : ''),
        };
      });
    },
    [submissionFormat],
  );

  useEffect(() => {
    if (availableLanguages.some((lang) => lang.id === selectedLanguage.id))
      return;
    const nextLanguage = availableLanguages[0];
    setSelectedLanguage(nextLanguage);
    setFiles((prev) => {
      const nextFiles = buildFilesForLanguage(nextLanguage, prev);
      if (nextFiles.length > 0) {
        setActiveFileId(nextFiles[0].id);
      }
      return nextFiles;
    });
  }, [availableLanguages, buildFilesForLanguage, selectedLanguage.id]);

  // Multi-file tabs state
  const [files, setFiles] = useState<EditorFile[]>(() => {
    const savedFiles = storageKey
      ? loadLanguageFiles(storageKey, selectedLanguage.id).map((file) => ({
          id: nextFileId(),
          filename: file.filename,
          content: file.content,
        }))
      : [];

    const initial = buildFilesForLanguage(selectedLanguage, savedFiles);
    return initial;
  });

  const [activeFileId, setActiveFileId] = useState<string>(() => {
    if (!storageKey || files.length === 0) return files[0]?.id ?? nextFileId();
    const savedActiveFilename = localStorage.getItem(
      getLanguageStorageKeys(storageKey, selectedLanguage.id).activeFile,
    );
    if (!savedActiveFilename) return files[0].id;
    const found = files.find((file) => file.filename === savedActiveFilename);
    return found?.id ?? files[0].id;
  });

  const activeFile = files.find((f) => f.id === activeFileId) ?? files[0];

  const editorRef = useRef<editor.IStandaloneCodeEditor | null>(null);
  const fileInputRef = useRef<HTMLInputElement | null>(null);
  const vimStatusRef = useRef<HTMLDivElement | null>(null);
  const [editorInstance, setEditorInstance] =
    useState<editor.IStandaloneCodeEditor | null>(null);
  const [monacoInstance, setMonacoInstance] = useState<Monaco>(null);
  const [keybindingMode, setKeybindingMode] = useEditorKeybindings(
    editorInstance,
    monacoInstance,
    vimStatusRef,
  );
  const { theme } = useTheme();

  // Reset editor state when storageKey changes (switching problems)
  const prevStorageKey = useRef(storageKey);
  useEffect(() => {
    if (storageKey === prevStorageKey.current) return;
    prevStorageKey.current = storageKey;

    // Load saved language
    let lang = availableLanguages[0];
    if (storageKey) {
      const savedLang = localStorage.getItem(
        getStorageKeys(storageKey).selectedLanguage,
      );
      if (savedLang) {
        const found = availableLanguages.find((l) => l.id === savedLang);
        if (found) lang = found;
      }
    }
    setSelectedLanguage(lang);

    const savedFiles = storageKey
      ? loadLanguageFiles(storageKey, lang.id).map((file) => ({
          id: nextFileId(),
          filename: file.filename,
          content: file.content,
        }))
      : [];

    const withSaved = buildFilesForLanguage(lang, savedFiles);
    setFiles(withSaved);
    if (withSaved.length > 0) {
      const savedActiveFilename = storageKey
        ? localStorage.getItem(
            getLanguageStorageKeys(storageKey, lang.id).activeFile,
          )
        : null;
      const active = savedActiveFilename
        ? withSaved.find((file) => file.filename === savedActiveFilename)
        : undefined;
      setActiveFileId(active?.id ?? withSaved[0].id);
    }
  }, [availableLanguages, buildFilesForLanguage, storageKey]);

  // Auto-save all files and current language to localStorage
  useEffect(() => {
    if (!storageKey) return;
    const keys = getStorageKeys(storageKey);

    saveLanguageFiles(storageKey, selectedLanguage.id, files);

    const active = files.find((file) => file.id === activeFileId) ?? files[0];
    if (active) {
      localStorage.setItem(
        getLanguageStorageKeys(storageKey, selectedLanguage.id).activeFile,
        active.filename,
      );
    }

    localStorage.setItem(keys.selectedLanguage, selectedLanguage.id);
  }, [storageKey, files, selectedLanguage, activeFileId]);

  const handleLanguageChange = useCallback(
    (language: Language) => {
      setSelectedLanguage(language);
      const persisted = storageKey
        ? loadLanguageFiles(storageKey, language.id).map((file) => ({
            id: nextFileId(),
            filename: file.filename,
            content: file.content,
          }))
        : [];
      setFiles((prev) => {
        const source = persisted.length > 0 ? persisted : prev;
        const next = buildFilesForLanguage(language, source);
        if (next.length > 0) {
          const savedActiveFilename = storageKey
            ? localStorage.getItem(
                getLanguageStorageKeys(storageKey, language.id).activeFile,
              )
            : null;
          const active = savedActiveFilename
            ? next.find((file) => file.filename === savedActiveFilename)
            : undefined;
          setActiveFileId(active?.id ?? next[0].id);
        }
        return next;
      });
    },
    [buildFilesForLanguage, storageKey],
  );

  const updateFileContent = useCallback((fileId: string, content: string) => {
    setFiles((prev) =>
      prev.map((f) => (f.id === fileId ? { ...f, content } : f)),
    );
  }, []);

  const closeFile = useCallback(
    (fileId: string) => {
      if (isSubmissionFormatLocked) return;
      setFiles((prev) => {
        if (prev.length <= 1) return prev; // Don't close last file
        const next = prev.filter((f) => f.id !== fileId);
        if (activeFileId === fileId) {
          setActiveFileId(next[0].id);
        }
        return next;
      });
    },
    [activeFileId, isSubmissionFormatLocked],
  );

  const addFiles = useCallback(
    (newFiles: EditorFile[]) => {
      if (newFiles.length === 0) return;

      // Detect language from uploaded files
      const detectedLang = detectLanguageFromFiles(newFiles);
      if (detectedLang) {
        const lang = availableLanguages.find((l) => l.id === detectedLang);
        if (lang) setSelectedLanguage(lang);
      }

      setFiles((prev) => {
        // If there's only the default template file, replace it
        const currentDefault = getDefaultFilename(
          selectedLanguage.id,
          submissionFormat,
        );
        const isDefault =
          prev.length === 1 &&
          prev[0].content === selectedLanguage.template &&
          prev[0].filename === currentDefault;

        const base = isDefault ? [] : prev;
        return [...base, ...newFiles];
      });

      // Activate the first new file
      setActiveFileId(newFiles[0].id);
    },
    [availableLanguages, selectedLanguage, submissionFormat],
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
    if (isSubmissionFormatLocked) return;
    const ext =
      getDefaultFilename(selectedLanguage.id, submissionFormat)
        .split('.')
        .pop() ?? 'txt';
    // Find a unique name like "file1.cpp", "file2.cpp", etc.
    let index = 1;
    const existingNames = new Set(files.map((f) => f.filename));
    while (existingNames.has(`file${index}.${ext}`)) index++;
    const filename = `file${index}.${ext}`;
    const newFile: EditorFile = { id: nextFileId(), filename, content: '' };
    setFiles((prev) => [...prev, newFile]);
    setActiveFileId(newFile.id);
  }, [isSubmissionFormatLocked, selectedLanguage, submissionFormat, files]);

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

  const handleEditorDidMount = (
    ed: editor.IStandaloneCodeEditor,
    monaco: Monaco,
  ) => {
    editorRef.current = ed;
    setEditorInstance(ed);
    setMonacoInstance(monaco);
  };

  const handleDrop = useCallback(
    (e: React.DragEvent) => {
      // Only intercept file drops, let dockview panel drags pass through
      if (!e.dataTransfer.types.includes('Files')) return;
      e.preventDefault();
      e.stopPropagation();
      if (e.dataTransfer.files.length > 0) {
        processUploadedFiles(e.dataTransfer.files);
      }
    },
    [processUploadedFiles],
  );

  const handleDragOver = useCallback((e: React.DragEvent) => {
    if (!e.dataTransfer.types.includes('Files')) return;
    e.preventDefault();
    e.stopPropagation();
  }, []);

  const toolbar = (
    <div className="flex items-center gap-1">
      <Slot
        name="problem-detail.editor.toolbar"
        as="div"
        className="flex items-center gap-1"
      />
      {contestTypes && contestTypes.length > 0 && onContestTypeChange && (
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button
              variant="ghost"
              size="sm"
              className="h-7 px-2 text-xs gap-1 text-muted-foreground"
            >
              {contestType ?? 'standard'}
              <ChevronDown className="h-3 w-3" />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="start">
            {contestTypes.map((ct) => (
              <DropdownMenuItem
                key={ct}
                onClick={() => onContestTypeChange(ct)}
              >
                {ct}
              </DropdownMenuItem>
            ))}
          </DropdownMenuContent>
        </DropdownMenu>
      )}
      <Button
        variant="ghost"
        size="sm"
        className="h-7 w-7 p-0 text-muted-foreground"
        onClick={handleUploadClick}
        title={t('editor.upload')}
      >
        <Upload className="h-3.5 w-3.5" />
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
          className="h-7 w-7 p-0 text-muted-foreground"
          onClick={onToggleFullscreen}
          aria-label={t('editor.toggleFullscreen')}
        >
          {isFullscreen ? (
            <Minimize2 className="h-3.5 w-3.5" />
          ) : (
            <Maximize2 className="h-3.5 w-3.5" />
          )}
        </Button>
      )}
      <div className="h-4 w-px bg-border mx-1" />
      <KeybindingModeDropdown
        mode={keybindingMode}
        onChange={setKeybindingMode}
      />
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button
            variant="ghost"
            size="sm"
            className="h-7 px-2 text-xs gap-1 font-medium"
          >
            {selectedLanguage.name}
            <ChevronDown className="h-3 w-3" />
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end">
          {availableLanguages.map((lang) => (
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
  );

  const fileTabs = (
    <div className="flex items-center gap-0 overflow-x-auto border-b">
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
          {files.length > 1 && !isSubmissionFormatLocked && (
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
      {!isSubmissionFormatLocked && (
        <button
          type="button"
          onClick={addNewFile}
          title={t('editor.addFile')}
          className="flex items-center px-2 py-1.5 text-muted-foreground hover:text-foreground transition-colors border-b-2 border-transparent"
        >
          <Plus className="h-3.5 w-3.5" />
        </button>
      )}
    </div>
  );

  const editorArea = (
    <div className="flex-1 min-h-0 border rounded-lg overflow-hidden flex flex-col">
      <div className="flex-1 min-h-0">
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
      <div
        ref={vimStatusRef}
        className={`vim-status-bar h-6 shrink-0 border-t border-border bg-muted/40 px-3 font-mono text-[11px] leading-6 text-muted-foreground ${keybindingMode !== 'vim' ? 'hidden' : ''}`}
      />
    </div>
  );

  const actionButtons = (
    <div className="flex gap-2 justify-end">
      <Button variant="outline" onClick={handleRun}>
        <Play className="mr-2 h-4 w-4" />
        {t('editor.run')}
      </Button>
      <Button onClick={handleSubmit}>{t('editor.submit')}</Button>
    </div>
  );

  return (
    <div
      className="h-full flex flex-col"
      onDrop={handleDrop}
      onDragOver={handleDragOver}
    >
      <div className="flex items-center justify-between px-2 py-1.5 shrink-0 border-b">
        {toolbar}
      </div>
      {fileTabs}
      <div className="flex-1 flex flex-col gap-3 p-3 min-h-0">
        {editorArea}
        {actionButtons}
      </div>
    </div>
  );
}
