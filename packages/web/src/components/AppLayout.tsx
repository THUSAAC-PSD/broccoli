import { Slot } from '@broccoli/web-sdk/slot';
import {
  SidebarInset,
  SidebarProvider,
  SidebarTrigger,
} from '@broccoli/web-sdk/ui';

import { Sidebar } from './Sidebar';

interface AppLayoutProps {
  children: React.ReactNode;
}

export function AppLayout({ children }: AppLayoutProps) {
  return (
    <Slot name="app.root" as="div">
      <SidebarProvider>
        <Sidebar />
        <SidebarInset>
          <div className="fixed left-0 top-1/2 z-50 -translate-y-1/2 md:hidden">
            <SidebarTrigger className="relative h-11 w-5 rounded-r-md rounded-l-none border border-l-0 border-border bg-muted/20 shadow-xs hover:bg-accent [&>svg]:hidden after:absolute after:inset-y-1.5 after:left-1/2 after:w-[2px] after:-translate-x-1/2 after:rounded-full after:bg-foreground/70" />
          </div>
          <div className="fixed bottom-8 right-8 z-50">
            <Slot name="app.NotificationButton" as="div" />
          </div>
          <div className="flex flex-1 flex-col">{children}</div>
        </SidebarInset>
      </SidebarProvider>
      <Slot name="app.overlay" as="div" />
    </Slot>
  );
}
