import { useRegistries } from '@broccoli/web-sdk/hooks';
import { useTranslation } from '@broccoli/web-sdk/i18n';

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

      <select
        value={selectedValue}
        onChange={(event) => onChange(event.target.value)}
        disabled={isLoading}
        className="flex h-9 w-full rounded-md border border-input bg-transparent px-3 py-1 text-sm shadow-sm transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
      >
        <option value="">
          {isLoading
            ? t('admin.loading')
            : t('admin.submissionFormat.language')}
        </option>
        {languages.map((lang) => (
          <option key={lang.id} value={lang.id}>
            {lang.name}
          </option>
        ))}
        {selectedValue && !hasCurrentValue && !isLoading && (
          <option value={selectedValue}>{selectedValue}</option>
        )}
      </select>
    </div>
  );
}
