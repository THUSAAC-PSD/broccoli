import { usePluginRegistry } from '@broccoli/sdk/react';
import { matchPath, useLocation } from 'react-router';
import { ErrorCatcher } from '@/components/ErrorCatcher';

/**
 * ExtensionPage
 * Handles dynamic routing for plugins.
 * It matches the current URL against registered plugin routes.
 */
export default function ExtensionPage() {
  const { routes, components } = usePluginRegistry();
  const location = useLocation();

  // TODO: Show a spinner or skeleton if plugins are still loading

  // Find the first route that matches the current pathname
  // We use matchPath to support parameters like /contest/:id
  const matchedRoute = routes.find((route) =>
    matchPath(
      { path: route.path, caseSensitive: false, end: true },
      location.pathname,
    ),
  );

  if (!matchedRoute) {
    // TODO: Render a proper 404 Not Found component
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

  // Render the plugin page
  // TODO: pass route metadata
  return <Component />;
}
