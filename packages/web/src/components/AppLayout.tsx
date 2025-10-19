import {
  SidebarProvider,
  SidebarInset,
} from '@/components/ui/sidebar';
import { Sidebar } from './Sidebar';

interface AppLayoutProps {
  children: React.ReactNode;
}

import { Navbar } from '@/components/Navbar';

export function AppLayout({ children }: AppLayoutProps) {
  return (
    <>
      <SidebarProvider>
        <Sidebar />
        <SidebarInset>
          <Navbar />
          <div className="flex flex-1 flex-col">{children}</div>
        </SidebarInset>
      </SidebarProvider>
    </>
  );
}
