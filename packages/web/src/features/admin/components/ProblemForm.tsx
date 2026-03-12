import { useTranslation } from '@broccoli/web-sdk/i18n';
import { Input, Label, Separator } from '@broccoli/web-sdk/ui';

import { MarkdownEditor } from '@/components/MarkdownEditor';
import { SwitchField } from '@/features/admin/components/SwitchField';

export interface ProblemFormData {
  title: string;
  content: string;
  timeLimit: number;
  memoryLimit: number;
  showTestDetails: boolean;
}

interface ProblemFormProps {
  data: ProblemFormData;
  onChange: (data: ProblemFormData) => void;
}

export function ProblemForm({ data, onChange }: ProblemFormProps) {
  const { t } = useTranslation();

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
    </>
  );
}
