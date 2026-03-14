import '@/App.css';

import { ApiClientProvider } from '@broccoli/web-sdk/api';
import { I18nProvider } from '@broccoli/web-sdk/i18n';
import type { LazyPluginLoader } from '@broccoli/web-sdk/plugin';
import { PluginRegistryProvider } from '@broccoli/web-sdk/plugin';
import { SidebarStateProvider } from '@broccoli/web-sdk/sidebar';
import { ThemeProvider } from '@broccoli/web-sdk/theme';
import { Toaster } from '@broccoli/web-sdk/ui';
import { QueryClientProvider } from '@tanstack/react-query';
import { useEffect } from 'react';
import { Links, Meta, Outlet, Scripts, ScrollRestoration } from 'react-router';

import { AppLayout } from '@/components/AppLayout';
import { SlotPermissionsBridge } from '@/components/SlotPermissionsBridge';
import { appConfig } from '@/config';
import { AuthProvider } from '@/features/auth/components/AuthProvider';
import { ContestProvider } from '@/features/contest/contexts/contest-context';
import { en } from '@/lib/i18n/en';
import { queryClient } from '@/lib/query-client';

// Lazy-loaded plugins — each is code-split into its own chunk by Vite.
const lazyPlugins: LazyPluginLoader[] = [];

export function Layout({ children }: { children: React.ReactNode }) {
  useEffect(() => {
    document.documentElement.style.opacity = '';
  }, []);

  return (
    <html suppressHydrationWarning>
      <head>
        <meta charSet="UTF-8" />
        <meta name="viewport" content="width=device-width, initial-scale=1.0" />
        <Meta />
        <Links />
        <script
          dangerouslySetInnerHTML={{
            __html: `(function(){var t=localStorage.getItem('theme');if(!t)t=matchMedia('(prefers-color-scheme:dark)').matches?'dark':'light';document.documentElement.classList.add(t);var l=localStorage.getItem('broccoli-locale');if(l&&l!=='en')document.documentElement.style.opacity='0'})()`,
          }}
        />
      </head>
      <body suppressHydrationWarning>
        <QueryClientProvider client={queryClient}>
          <ApiClientProvider
            baseUrl={appConfig.api.baseUrl}
            authTokenKey={appConfig.api.authTokenKey}
          >
            <I18nProvider defaultLocale="en" coreI18n={{ en }}>
              <ThemeProvider defaultTheme="light" storageKey="theme">
                <AuthProvider>
                  <ContestProvider>
                    <SidebarStateProvider
                      defaultState="expanded"
                      storageKey="sidebar-state"
                    >
                      <SlotPermissionsBridge>
                        <PluginRegistryProvider
                          backendUrl={appConfig.plugin.backendUrl}
                          lazyPlugins={lazyPlugins}
                        >
                          <AppLayout>{children}</AppLayout>
                          <Toaster richColors closeButton />
                        </PluginRegistryProvider>
                      </SlotPermissionsBridge>
                    </SidebarStateProvider>
                  </ContestProvider>
                </AuthProvider>
              </ThemeProvider>
            </I18nProvider>
          </ApiClientProvider>
        </QueryClientProvider>
        <ScrollRestoration />
        <Scripts />
      </body>
    </html>
  );
}

export default function Root() {
  return <Outlet />;
}
