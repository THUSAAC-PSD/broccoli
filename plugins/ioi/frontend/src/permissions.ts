import {
  CONTEST_MANAGE,
  SUBMISSION_VIEW_ALL,
} from '@broccoli/web-sdk/permissions';

const PRIVILEGED_SUBMISSION_PERMISSIONS = [
  CONTEST_MANAGE,
  SUBMISSION_VIEW_ALL,
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
