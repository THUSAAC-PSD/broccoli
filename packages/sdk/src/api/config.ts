export const AUTH_TOKEN_KEY = `broccoli_token`;

export const API_CONFIG = {
  BASE_URL: import.meta.env.VITE_API_BASE_URL || 'http://localhost:3000/api/v1',
  TIMEOUT: 10000,
} as const;
