import { Slot } from '@broccoli/sdk/react';
import { createRootRoute, Outlet } from '@tanstack/react-router';
import { TanStackRouterDevtools } from '@tanstack/react-router-devtools';

import { Navbar } from '@/components/Navbar';
import { Sidebar } from '@/components/Sidebar';
import { SidebarInset, SidebarProvider } from '@/components/ui/sidebar';

export const Route = createRootRoute({
  component: RootComponent,
});

function RootComponent() {
  return (
    <Slot name="app.root" as="div">
      <SidebarProvider>
        <Sidebar />
        <SidebarInset>
          <Navbar />
          <div className="flex flex-1 flex-col">
            <Outlet />
          </div>
        </SidebarInset>
      </SidebarProvider>
      <Slot name="app.overlay" as="div" />

      {/* Display router devtools only in development mode */}
      {import.meta.env.DEV && <TanStackRouterDevtools />}
    </Slot>
  );
}
