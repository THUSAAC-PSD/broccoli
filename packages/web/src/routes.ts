import { route, type RouteConfig } from '@react-router/dev/routes';
import { flatRoutes } from '@react-router/fs-routes';

export default [
  ...(await flatRoutes({
    ignoredRouteFiles: ['routes/extension.tsx'],
  })),
  route('*', 'routes/extension.tsx'),
] satisfies RouteConfig;
