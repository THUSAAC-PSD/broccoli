import { Slot } from '@broccoli/sdk/react';

import { Navbar } from '@/components/Navbar';
import { SidebarInset,SidebarProvider } from '@/components/ui/sidebar';

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
          <Navbar />
          <div className="flex flex-1 flex-col">{children}</div>
        </SidebarInset>
      </SidebarProvider>
      <Slot name="app.overlay" as="div" />
    </Slot>
  );
}
