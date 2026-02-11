import { useTranslation } from '@broccoli/sdk/i18n';

export function AmazingPage() {
  const { t } = useTranslation();

  return <div>{t('plugin.amazingButton.pageTitle')}</div>;
}
