import createClient from 'openapi-fetch';

import { API_CONFIG, AUTH_TOKEN_KEY } from './config';
import type { paths } from './schema';

export const api = createClient<paths>({
  baseUrl: API_CONFIG.BASE_URL,
  // TODO: headers
});

api.use({
  onRequest({ request }) {
    const token = localStorage.getItem(AUTH_TOKEN_KEY);
    if (token) {
      request.headers.set('Authorization', `Bearer ${token}`);
    }
    return request;
  },
  onResponse({ response }) {
    if (response.status === 401) {
      localStorage.removeItem(AUTH_TOKEN_KEY);
    }
    return response;
  },
});
