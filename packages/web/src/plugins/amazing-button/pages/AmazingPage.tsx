import { useTranslation } from '@broccoli/web-sdk/i18n';

export function AmazingPage() {
  const { t } = useTranslation();

  return <div>{t('plugin.amazingButton.pageTitle')}</div>;
}
