export type ParsedBulkUser = {
  username: string;
  password?: string;
};

export function normalizeBulkUsers(input: unknown): ParsedBulkUser[] {
  if (!Array.isArray(input)) {
    throw new Error('admin.bulkParticipantsInvalidJson');
  }

  const users: ParsedBulkUser[] = [];
  const seen = new Set<string>();

  for (const item of input) {
    let username = '';
    let password: string | undefined;

    if (typeof item === 'string') {
      username = item.trim();
    } else if (item && typeof item === 'object') {
      const record = item as { username?: unknown; password?: unknown };
      username =
        typeof record.username === 'string' ? record.username.trim() : '';
      if (record.password != null && typeof record.password !== 'string') {
        throw new Error('admin.bulkParticipantsInvalidPassword');
      }
      if (typeof record.password === 'string') {
        password = record.password;
      }
    }

    if (!username) {
      throw new Error('admin.bulkParticipantsInvalidUsername');
    }

    if (username.length > 32 || !/^[A-Za-z0-9_]+$/.test(username)) {
      throw new Error('admin.bulkParticipantsInvalidUsername');
    }

    if (
      password !== undefined &&
      (new TextEncoder().encode(password).length < 8 ||
        new TextEncoder().encode(password).length > 128)
    ) {
      throw new Error('admin.bulkParticipantsInvalidPassword');
    }

    const key = username.toLowerCase();
    if (seen.has(key)) {
      throw new Error('admin.bulkParticipantsDuplicate');
    }
    seen.add(key);

    users.push({ username, password });
  }

  if (users.length === 0) {
    throw new Error('admin.bulkParticipantsEmpty');
  }

  return users;
}
