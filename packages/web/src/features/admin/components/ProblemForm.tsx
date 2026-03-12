import { useApiClient } from '@broccoli/web-sdk/api';
import { useTranslation } from '@broccoli/web-sdk/i18n';
import {
  Button,
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
  Input,
  Label,
  Separator,
} from '@broccoli/web-sdk/ui';
import { useQuery } from '@tanstack/react-query';
import { ChevronDown, Plus, X } from 'lucide-react';
import { useMemo, useState } from 'react';

import { MarkdownEditor } from '@/components/MarkdownEditor';
import { SwitchField } from '@/features/admin/components/SwitchField';
import {
  fetchSupportedLanguages,
  type SupportedLanguage,
} from '@/features/problem/api/fetch-supported-languages';

export interface ProblemFormData {
  title: string;
  content: string;
  timeLimit: number;
  memoryLimit: number;
  showTestDetails: boolean;
  submissionFormat: Record<string, string[]>;
}

interface ProblemFormProps {
  data: ProblemFormData;
  onChange: (data: ProblemFormData) => void;
}

function fallbackDefaultFilename(languageId: string): string {
  const extMap: Record<string, string> = {
    cpp: 'cpp',
    c: 'c',
    python: 'py',
    java: 'java',
  };
  const ext = extMap[languageId] ?? 'txt';
  return `solution.${ext}`;
}

function getDefaultFilename(language: SupportedLanguage): string {
  return language.defaultFilename || fallbackDefaultFilename(language.id);
}

export function ProblemForm({ data, onChange }: ProblemFormProps) {
  const { t } = useTranslation();
  const apiClient = useApiClient();
  const { data: supportedLanguages = [] } = useQuery({
    queryKey: ['supported-languages'],
    queryFn: () => fetchSupportedLanguages(apiClient),
    staleTime: 5 * 60 * 1000,
  });
  const [selectedLanguage, setSelectedLanguage] = useState('');
  const [newFilenameByLanguage, setNewFilenameByLanguage] = useState<
    Record<string, string>
  >({});

  const handleTitleChange = (title: string) => {
    onChange({ ...data, title });
  };

  const handleContentChange = (content: string) => {
    onChange({ ...data, content });
  };

  const handleTimeLimitChange = (timeLimit: number) => {
    onChange({ ...data, timeLimit });
  };

  const handleMemoryLimitChange = (memoryLimit: number) => {
    onChange({ ...data, memoryLimit });
  };

  const handleShowTestDetailsChange = (showTestDetails: boolean) => {
    onChange({ ...data, showTestDetails });
  };

  const configuredLanguages = useMemo(
    () => Object.keys(data.submissionFormat),
    [data.submissionFormat],
  );

  const languageById = useMemo(
    () => new Map(supportedLanguages.map((lang) => [lang.id, lang])),
    [supportedLanguages],
  );

  const canAddLanguages = useMemo(
    () =>
      supportedLanguages.filter(
        (lang) => !configuredLanguages.includes(lang.id),
      ),
    [supportedLanguages, configuredLanguages],
  );

  const selectedLanguageLabel = useMemo(() => {
    if (!selectedLanguage) return t('admin.submissionFormat.language');
    const language = languageById.get(selectedLanguage);
    if (!language) return selectedLanguage;
    return `${language.name} (${language.id})`;
  }, [languageById, selectedLanguage, t]);

  const addLanguage = () => {
    const lang = selectedLanguage.trim();
    if (!lang || data.submissionFormat[lang]) return;
    const languageMeta = languageById.get(lang);
    const defaultFilename = languageMeta
      ? getDefaultFilename(languageMeta)
      : fallbackDefaultFilename(lang);
    onChange({
      ...data,
      submissionFormat: {
        ...data.submissionFormat,
        [lang]: [defaultFilename],
      },
    });
    setSelectedLanguage('');
  };

  const removeLanguage = (lang: string) => {
    const next = { ...data.submissionFormat };
    delete next[lang];
    onChange({ ...data, submissionFormat: next });
  };

  const addFilename = (lang: string) => {
    const filename = (newFilenameByLanguage[lang] ?? '').trim();
    if (!filename) return;
    const existing = data.submissionFormat[lang] ?? [];
    if (existing.includes(filename)) return;
    onChange({
      ...data,
      submissionFormat: {
        ...data.submissionFormat,
        [lang]: [...existing, filename],
      },
    });
    setNewFilenameByLanguage((prev) => ({ ...prev, [lang]: '' }));
  };

  const removeFilename = (lang: string, filename: string) => {
    const existing = data.submissionFormat[lang] ?? [];
    onChange({
      ...data,
      submissionFormat: {
        ...data.submissionFormat,
        [lang]: existing.filter((name) => name !== filename),
      },
    });
  };

  return (
    <>
      <div className="space-y-2">
        <Label htmlFor="problem-title">{t('admin.field.title')}</Label>
        <Input
          id="problem-title"
          value={data.title}
          onChange={(e) => handleTitleChange(e.target.value)}
          required
          maxLength={256}
          placeholder="Two Sum"
        />
      </div>

      <div className="space-y-2">
        <Label htmlFor="problem-content">{t('admin.field.content')}</Label>
        <MarkdownEditor
          id="problem-content"
          value={data.content}
          onChange={handleContentChange}
          minHeight={250}
          placeholder="Problem statement (Markdown supported)"
        />
      </div>

      <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
        <div className="space-y-2">
          <Label htmlFor="problem-time">{t('admin.field.timeLimit')}</Label>
          <Input
            id="problem-time"
            type="number"
            min={1}
            max={30000}
            value={data.timeLimit}
            onChange={(e) => handleTimeLimitChange(Number(e.target.value))}
            required
          />
        </div>
        <div className="space-y-2">
          <Label htmlFor="problem-memory">{t('admin.field.memoryLimit')}</Label>
          <Input
            id="problem-memory"
            type="number"
            min={1}
            max={1048576}
            value={data.memoryLimit}
            onChange={(e) => handleMemoryLimitChange(Number(e.target.value))}
            required
          />
        </div>
      </div>

      <Separator />

      <div className="space-y-3">
        <Label className="text-sm text-muted-foreground">
          {t('admin.field.options')}
        </Label>
        <SwitchField
          id="problem-test-details"
          label={t('admin.field.showTestDetails')}
          checked={data.showTestDetails}
          onCheckedChange={handleShowTestDetailsChange}
        />
      </div>

      <Separator />

      <div className="space-y-3">
        <Label className="text-sm text-muted-foreground">
          {t('admin.field.submissionFormat')}
        </Label>

        <div className="flex gap-2">
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button
                type="button"
                variant="outline"
                className="flex-1 justify-between"
                disabled={canAddLanguages.length === 0}
              >
                <span className="truncate">{selectedLanguageLabel}</span>
                <ChevronDown className="ml-2 h-4 w-4" />
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="start">
              {canAddLanguages.map((lang) => (
                <DropdownMenuItem
                  key={lang.id}
                  onClick={() => setSelectedLanguage(lang.id)}
                >
                  {lang.name} ({lang.id})
                </DropdownMenuItem>
              ))}
            </DropdownMenuContent>
          </DropdownMenu>
          <Button
            type="button"
            variant="outline"
            onClick={addLanguage}
            disabled={!selectedLanguage || canAddLanguages.length === 0}
          >
            <Plus className="h-4 w-4 mr-1" />
            {t('admin.submissionFormat.addLanguage')}
          </Button>
        </div>

        {configuredLanguages.length === 0 ? (
          <div className="text-sm text-muted-foreground">
            {t('admin.submissionFormat.empty')}
          </div>
        ) : (
          <div className="space-y-3">
            {configuredLanguages.map((lang) => (
              <div key={lang} className="rounded-md border p-3 space-y-2">
                <div className="flex items-center justify-between">
                  <div className="text-sm font-medium">{lang}</div>
                  <Button
                    type="button"
                    variant="ghost"
                    size="sm"
                    className="h-7 px-2 text-destructive hover:text-destructive"
                    onClick={() => removeLanguage(lang)}
                  >
                    <X className="h-3.5 w-3.5 mr-1" />
                    {t('admin.delete')}
                  </Button>
                </div>

                <div className="flex flex-wrap gap-2">
                  {(data.submissionFormat[lang] ?? []).map((filename) => (
                    <span
                      key={`${lang}-${filename}`}
                      className="inline-flex items-center gap-1 rounded-md border px-2 py-1 text-xs"
                    >
                      <span>{filename}</span>
                      <button
                        type="button"
                        onClick={() => removeFilename(lang, filename)}
                        className="text-muted-foreground hover:text-destructive"
                      >
                        <X className="h-3 w-3" />
                      </button>
                    </span>
                  ))}
                </div>

                <div className="flex gap-2">
                  <Input
                    value={newFilenameByLanguage[lang] ?? ''}
                    placeholder={t(
                      'admin.submissionFormat.filenamePlaceholder',
                    )}
                    onChange={(e) =>
                      setNewFilenameByLanguage((prev) => ({
                        ...prev,
                        [lang]: e.target.value,
                      }))
                    }
                  />
                  <Button
                    type="button"
                    variant="outline"
                    onClick={() => addFilename(lang)}
                  >
                    <Plus className="h-4 w-4 mr-1" />
                    {t('admin.submissionFormat.addFile')}
                  </Button>
                </div>
              </div>
            ))}
          </div>
        )}
      </div>
    </>
  );
}
