import { usePluginRegistry } from '@broccoli/sdk/react';
import { matchPath, useLocation } from 'react-router';

import { ErrorCatcher } from '@/components/ErrorCatcher';
import { Skeleton } from '@/components/ui/skeleton';

/**
 * ExtensionPage
 * Handles dynamic routing for plugins.
 * It matches the current URL against registered plugin routes.
 */
export default function ExtensionPage() {
  const { routes, components, isLoading } = usePluginRegistry();
  const location = useLocation();

  // Show loading state while plugins are still being loaded
  if (isLoading) {
    return (
      <div className="flex flex-col gap-6 p-6">
        <Skeleton className="h-8 w-48" />
        <Skeleton className="h-64 w-full" />
      </div>
    );
  }

  // Find the first route that matches the current pathname
  // We use matchPath to support parameters like /contest/:id
  const matchedRoute = routes.find((route) =>
    matchPath(
      { path: route.path, caseSensitive: false, end: true },
      location.pathname,
    ),
  );

  if (!matchedRoute) {
    return <ErrorCatcher code="404" />;
  }

  const Component = components[matchedRoute.component];

  if (!Component) {
    return (
      <div className="p-8 text-center text-destructive">
        <h1 className="text-xl font-bold">Component Error</h1>
        <p>
          Some plugin registered this route, but component{' '}
          <code>{matchedRoute.component}</code> is missing.
        </p>
      </div>
    );
  }

  // TODO: pass route metadata
  return <Component />;
}
