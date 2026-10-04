import assert from 'node:assert/strict';
import test from 'node:test';

import { normalizeBulkUsers } from './bulk-users.ts';

test('normalizes usernames while preserving supplied passwords exactly', () => {
  assert.deepEqual(
    normalizeBulkUsers([
      ' alice ',
      { username: 'bob', password: ' supplied_pass ' },
      { username: 'carol', password: null },
    ]),
    [
      { username: 'alice', password: undefined },
      { username: 'bob', password: ' supplied_pass ' },
      { username: 'carol', password: undefined },
    ],
  );
});

test('rejects invalid and duplicate usernames before an import', () => {
  for (const input of [
    [],
    {},
    ['valid', 'bad-name'],
    ['duplicate', ' Duplicate '],
    [{ username: 'x'.repeat(33) }],
    [''],
  ]) {
    assert.throws(() => normalizeBulkUsers(input));
  }
});

test('explicit invalid passwords never become generated passwords', () => {
  for (const password of ['', 'short', 123, false, 'é'.repeat(65)]) {
    assert.throws(
      () => normalizeBulkUsers([{ username: 'alice', password }]),
      /admin.bulkParticipantsInvalidPassword/,
    );
  }
  assert.equal(
    normalizeBulkUsers([{ username: 'alice', password: 'é'.repeat(64) }])[0]
      .password,
    'é'.repeat(64),
  );
});
