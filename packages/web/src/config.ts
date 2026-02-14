export interface AppConfig {
  api: {
    baseUrl: string;
    authTokenKey: string;
  };
  plugin: {
    backendUrl: string;
  };
}

export const appConfig: AppConfig = {
  api: {
    baseUrl:
      import.meta.env.VITE_API_BASE_URL || 'http://localhost:3000/api/v1',
    authTokenKey: 'broccoli_token',
  },
  plugin: {
    backendUrl:
      import.meta.env.VITE_PLUGIN_BACKEND_URL || 'http://localhost:3000',
  },
};
