import type { components } from '@/api/schema';

export type User = components['schemas']['MeResponse'];
export type LoginRequest = components['schemas']['LoginRequest'];

export const AUTH_SESSION_EXPIRED_EVENT = 'broccoli:auth:session-expired';

export const USER_PERMISSIONS = [
  'submission:view_all',
  'submission:rejudge',
  'problem:create',
  'problem:edit',
  'problem:delete',
  'contest:create',
  'contest:manage',
  'contest:delete',
  'plugin:manage',
  'user:manage',
  'dlq:manage',
];
