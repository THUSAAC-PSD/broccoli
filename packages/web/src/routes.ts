import { layout, route, type RouteConfig } from '@react-router/dev/routes';
import { flatRoutes } from '@react-router/fs-routes';

export default [
  layout('routes/_app.tsx', [
    ...(await flatRoutes({
      ignoredRouteFiles: ['routes/extension.tsx', 'routes/_app.tsx'],
    })),
    route('*', 'routes/extension.tsx'),
  ]),
] satisfies RouteConfig;
