import { useTranslation } from '@broccoli/web-sdk/i18n';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@broccoli/web-sdk/ui';

import { RolesManagementTab } from '@/features/user-management/components/RolesManagementTab';
import { UsersManagementTab } from '@/features/user-management/components/UsersManagementTab';

interface UserManagementTabsProps {
  canManageRoles: boolean;
  canManageUsers: boolean;
}

export function UserManagementTabs({
  canManageRoles,
  canManageUsers,
}: UserManagementTabsProps) {
  const { t } = useTranslation();

  if (canManageRoles && canManageUsers) {
    return (
      <Tabs defaultValue="users" className="space-y-4">
        <TabsList>
          <TabsTrigger value="users">{t('users.tabs.users')}</TabsTrigger>
          <TabsTrigger value="roles">{t('users.tabs.roles')}</TabsTrigger>
        </TabsList>
        <TabsContent value="users" className="mt-0">
          <UsersManagementTab />
        </TabsContent>
        <TabsContent value="roles" className="mt-0">
          <RolesManagementTab />
        </TabsContent>
      </Tabs>
    );
  }

  if (canManageUsers) {
    return <UsersManagementTab />;
  }

  if (canManageRoles) {
    return <RolesManagementTab />;
  }

  return null;
}
