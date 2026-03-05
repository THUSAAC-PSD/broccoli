import { Slot } from '@broccoli/sdk/react';

import { SidebarInset, SidebarProvider } from '@/components/ui/sidebar';

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
