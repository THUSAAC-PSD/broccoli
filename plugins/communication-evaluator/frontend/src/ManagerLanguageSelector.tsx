import { useRegistries } from '@broccoli/web-sdk/hooks';
import { useTranslation } from '@broccoli/web-sdk/i18n';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@broccoli/web-sdk/ui';

interface ManagerLanguageSelectorProps {
  value: unknown;
  schema: { title?: string; description?: string };
  onChange: (value: unknown) => void;
}

export function ManagerLanguageSelector({
  value,
  schema,
  onChange,
}: ManagerLanguageSelectorProps) {
  const { t } = useTranslation();
  const { data, isLoading } = useRegistries();
  const languages = data?.languages ?? [];

  const selectedValue = typeof value === 'string' ? value : '';

  // If current value is not in the list, show it as fallback to prevent data loss
  const hasCurrentValue =
    selectedValue !== '' && languages.some((lang) => lang.id === selectedValue);

  return (
    <div className="flex flex-col gap-2">
      <label className="text-[11px] font-semibold uppercase tracking-wider opacity-55">
        {schema.title ?? t('admin.additionalFiles.language')}
      </label>
      {schema.description && (
        <p className="text-xs opacity-50 m-0 leading-normal">
          {schema.description}
        </p>
      )}

      <Select
        value={selectedValue || undefined}
        onValueChange={(v) => onChange(v)}
        disabled={isLoading}
      >
        <SelectTrigger>
          <SelectValue
            placeholder={
              isLoading
                ? t('admin.loading')
                : t('admin.submissionFormat.language')
            }
          />
        </SelectTrigger>
        <SelectContent>
          {languages.map((lang) => (
            <SelectItem key={lang.id} value={lang.id}>
              {lang.name}
            </SelectItem>
          ))}
          {selectedValue && !hasCurrentValue && !isLoading && (
            <SelectItem value={selectedValue}>{selectedValue}</SelectItem>
          )}
        </SelectContent>
      </Select>
    </div>
  );
}
