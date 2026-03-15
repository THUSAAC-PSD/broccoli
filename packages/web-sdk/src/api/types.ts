import type { Client } from 'openapi-fetch';

import type { paths } from '@/api/schema';

export type ApiClient = Client<paths>;
