import { useTranslation } from '@broccoli/sdk/i18n';
import { Languages } from 'lucide-react';

import { SidebarMenuButton, SidebarMenuItem } from '@/components/ui/sidebar';

const LOCALE_LABELS: Record<string, string> = {
  en: 'English',
  'zh-CN': '中文',
};

export function LocaleSwitcher() {
  const { locale, setLocale, availableLocales, t } = useTranslation();

  const cycleLocale = () => {
    const currentIndex = availableLocales.indexOf(locale);
    const nextIndex = (currentIndex + 1) % availableLocales.length;
    setLocale(availableLocales[nextIndex]);
  };

  const currentLabel = LOCALE_LABELS[locale] ?? locale;

  return (
    <SidebarMenuItem>
      <SidebarMenuButton
        onClick={cycleLocale}
        tooltip={t('locale.switch')}
        className="bg-sidebar-accent/50 hover:bg-sidebar-accent"
      >
        <Languages />
        <span>{currentLabel}</span>
      </SidebarMenuButton>
    </SidebarMenuItem>
  );
}
