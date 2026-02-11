import { useTranslation } from '@broccoli/sdk/i18n';
import { Sparkles } from 'lucide-react';

import { SidebarMenuButton, SidebarMenuItem } from '@/components/ui/sidebar';

export function AmazingButton() {
  const { t } = useTranslation();

  return (
    <SidebarMenuItem>
      <SidebarMenuButton onClick={() => alert(t('plugin.amazingButton.alert'))}>
        <Sparkles />
        <span>{t('plugin.amazingButton.label')}</span>
      </SidebarMenuButton>
    </SidebarMenuItem>
  );
}
