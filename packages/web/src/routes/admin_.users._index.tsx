import { useAuth } from '@broccoli/web-sdk/auth';
import { useTranslation } from '@broccoli/web-sdk/i18n';
import { Users } from 'lucide-react';

import { PageLayout } from '@/components/PageLayout';
import { Unauthorized } from '@/components/Unauthorized';
import { UserManagementTabs } from '@/features/user-management/components/UserManagementTabs';

export default function UserManagementPage() {
  const { t } = useTranslation();
  const { user } = useAuth();

  const canManageUsers = !!user?.permissions.includes('user:manage');
  const canManageRoles = !!user?.permissions.includes('role:manage');

  if (!user || (!canManageUsers && !canManageRoles)) {
    return <Unauthorized />;
  }

  return (
    <PageLayout
      pageId="users"
      title={t('users.title')}
      subtitle={t('users.subtitle')}
      icon={<Users className="h-6 w-6 text-primary" />}
    >
      <UserManagementTabs
        canManageUsers={canManageUsers}
        canManageRoles={canManageRoles}
      />
    </PageLayout>
  );
}
