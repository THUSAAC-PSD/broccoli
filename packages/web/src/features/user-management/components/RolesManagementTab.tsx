import { useTranslation } from '@broccoli/web-sdk/i18n';
import {
  Badge,
  Button,
  DataTable,
  type DataTableColumn,
} from '@broccoli/web-sdk/ui';
import { ShieldCheck } from 'lucide-react';
import { useMemo, useState } from 'react';

import { fetchRolesTable } from '@/features/user-management/api/fetch-roles-table';
import { ManageRolePermissionsDialog } from '@/features/user-management/components/ManageRolePermissionsDialog';
import type { RolePermissionsRow } from '@/features/user-management/types';
import { useTableSearchParams } from '@/hooks/use-table-search-params';

function useRoleColumns(
  onManagePermissions: (role: RolePermissionsRow) => void,
): DataTableColumn<RolePermissionsRow>[] {
  const { t } = useTranslation();

  return useMemo(
    () => [
      {
        accessorKey: 'role',
        header: t('users.roles.role'),
        sortKey: 'role',
        cell: ({ row }) => (
          <span className="font-medium text-foreground">
            {row.original.role}
          </span>
        ),
      },
      {
        accessorKey: 'permission_count',
        header: t('users.roles.permissionCount'),
        sortKey: 'permission_count',
        size: 120,
      },
      {
        accessorKey: 'permissions',
        header: t('users.roles.permissions'),
        cell: ({ row }) => {
          const permissions = row.original.permissions;
          if (permissions.length === 0) {
            return (
              <span className="text-xs text-muted-foreground">
                {t('users.roles.noPermissions')}
              </span>
            );
          }

          return (
            <div className="flex flex-wrap gap-1">
              {permissions.slice(0, 4).map((permission) => (
                <Badge
                  key={permission}
                  variant="outline"
                  className="font-mono text-[11px]"
                >
                  {permission}
                </Badge>
              ))}
              {permissions.length > 4 && (
                <Badge variant="secondary" className="text-[11px]">
                  +{permissions.length - 4}
                </Badge>
              )}
            </div>
          );
        },
      },
      {
        id: 'actions',
        header: '',
        size: 200,
        cell: ({ row }) => (
          <div className="flex justify-end">
            <Button
              type="button"
              variant="outline"
              size="sm"
              onClick={() => onManagePermissions(row.original)}
            >
              <ShieldCheck className="h-4 w-4" />
              {t('users.roles.managePermissions')}
            </Button>
          </div>
        ),
      },
    ],
    [onManagePermissions, t],
  );
}

export function RolesManagementTab() {
  const { t } = useTranslation();
  const table = useTableSearchParams({
    defaultSortBy: 'role',
    defaultSortOrder: 'asc',
  });

  const [managingRole, setManagingRole] = useState<RolePermissionsRow>();
  const [dialogOpen, setDialogOpen] = useState(false);

  function handleManagePermissions(role: RolePermissionsRow) {
    setManagingRole(role);
    setDialogOpen(true);
  }

  const columns = useRoleColumns(handleManagePermissions);

  return (
    <>
      <DataTable
        columns={columns}
        queryKey={['admin-role-permissions']}
        fetchFn={fetchRolesTable}
        searchable
        searchPlaceholder={t('users.roles.searchPlaceholder')}
        defaultPerPage={20}
        defaultSortBy="role"
        defaultSortOrder="asc"
        emptyMessage={t('users.roles.empty')}
        state={table.state}
        onPageChange={table.setPage}
        onSearchChange={table.setSearch}
        onSortChange={table.setSort}
      />
      <ManageRolePermissionsDialog
        role={managingRole}
        open={dialogOpen}
        onOpenChange={setDialogOpen}
      />
    </>
  );
}
