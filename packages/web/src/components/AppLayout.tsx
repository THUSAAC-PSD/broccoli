import { Slot } from '@broccoli/sdk/react';

import {
  SidebarInset,
  SidebarProvider,
  SidebarTrigger,
} from '@/components/ui/sidebar';

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
          <div className="fixed left-4 top-4 z-50 md:hidden">
            <SidebarTrigger className="h-9 w-9 rounded-md border bg-background/90 shadow-sm backdrop-blur" />
          </div>
          <div className="fixed bottom-8 right-8 z-50">
            <Slot name="app.NotificationButton" as="div" />
          </div>
          <div className="flex flex-1 flex-col pt-12 container mx-auto px-4">
            {children}
          </div>
        </SidebarInset>
      </SidebarProvider>
      <Slot name="app.overlay" as="div" />
    </Slot>
  );
}
