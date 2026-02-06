import { Sparkles } from 'lucide-react';

import { SidebarMenuButton,SidebarMenuItem } from '@/components/ui/sidebar';

export function AmazingButton() {
  return (
    <SidebarMenuItem>
      <SidebarMenuButton onClick={() => alert('Amazing!')}>
        <Sparkles />
        <span>Amazing Button</span>
      </SidebarMenuButton>
    </SidebarMenuItem>
  );
}
