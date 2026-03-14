const PRIVILEGED_SUBMISSION_PERMISSIONS = [
  'contest:manage',
  'submission:view_all',
] as const;

export function canViewPrivilegedSubmissionFeedback(
  permissions?: readonly string[] | null,
): boolean {
  if (!permissions || permissions.length === 0) {
    return false;
  }

  return PRIVILEGED_SUBMISSION_PERMISSIONS.some((permission) =>
    permissions.includes(permission),
  );
}
