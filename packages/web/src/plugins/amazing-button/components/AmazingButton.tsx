import { SidebarMenuItem, SidebarMenuButton } from '@/components/ui/sidebar';
import { Sparkles } from 'lucide-react';

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
