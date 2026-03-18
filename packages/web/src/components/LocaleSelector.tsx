import { useTranslation } from '@broccoli/web-sdk/i18n';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
  SidebarMenuButton,
  SidebarMenuItem,
} from '@broccoli/web-sdk/ui';
import { ChevronUp, Languages } from 'lucide-react';

const LOCALE_LABELS: Record<string, string> = {
  en: 'English',
  'zh-CN': '中文',
};

export function LocaleSelector() {
  const { locale, setLocale, availableLocales } = useTranslation();

  const currentLabel = LOCALE_LABELS[locale] ?? locale;

  return (
    <SidebarMenuItem>
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <SidebarMenuButton>
            <Languages className="h-4 w-4" />
            <span className="flex-1">{currentLabel}</span>
            <ChevronUp className="ml-auto h-4 w-4" />
          </SidebarMenuButton>
        </DropdownMenuTrigger>
        <DropdownMenuContent
          side="top"
          className="w-(--radix-dropdown-menu-trigger-width)"
        >
          {availableLocales.map((loc) => (
            <DropdownMenuItem
              key={loc}
              onClick={() => setLocale(loc)}
              className={locale === loc ? 'bg-accent' : ''}
            >
              {LOCALE_LABELS[loc] ?? loc}
            </DropdownMenuItem>
          ))}
        </DropdownMenuContent>
      </DropdownMenu>
    </SidebarMenuItem>
  );
}
