import { useApiClient } from '@broccoli/web-sdk/api';
import { useTranslation } from '@broccoli/web-sdk/i18n';
import {
  Button,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  Input,
  Label,
} from '@broccoli/web-sdk/ui';
import { useQueryClient } from '@tanstack/react-query';
import { Loader2 } from 'lucide-react';
import { useEffect, useState } from 'react';
import { toast } from 'sonner';

import type { ManagedUserRow } from '@/features/user-management/types';
import { extractErrorMessage } from '@/lib/extract-error';

interface EditUserDialogProps {
  user?: ManagedUserRow;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

export function EditUserDialog({
  user,
  open,
  onOpenChange,
}: EditUserDialogProps) {
  const { t } = useTranslation();
  const apiClient = useApiClient();
  const queryClient = useQueryClient();

  const [username, setUsername] = useState('');
  const [password, setPassword] = useState('');
  const [isSubmitting, setIsSubmitting] = useState(false);

  useEffect(() => {
    if (!open || !user) return;
    setUsername(user.username);
    setPassword('');
  }, [open, user]);

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    if (!user) return;

    const nextUsername = username.trim();
    const nextPassword = password.trim();
    const body: { username?: string; password?: string } = {};

    if (nextUsername && nextUsername !== user.username) {
      body.username = nextUsername;
    }

    if (nextPassword) {
      body.password = nextPassword;
    }

    if (Object.keys(body).length === 0) {
      toast.error(t('users.users.noChanges'));
      return;
    }

    setIsSubmitting(true);
    const { error } = await apiClient.PATCH('/users/{id}', {
      params: { path: { id: user.id } },
      body,
    });
    setIsSubmitting(false);

    if (error) {
      toast.error(extractErrorMessage(error, t('users.users.updateUserError')));
      return;
    }

    toast.success(t('users.users.updateUserSuccess'));
    queryClient.invalidateQueries({ queryKey: ['admin-users'] });
    queryClient.invalidateQueries({ queryKey: ['admin-user-detail', user.id] });
    onOpenChange(false);
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-md">
        <DialogHeader>
          <DialogTitle>{t('users.users.editUser')}</DialogTitle>
          <DialogDescription>
            {user
              ? t('users.users.editUserDescription', {
                  username: user.username,
                })
              : t('users.users.editUserDescriptionFallback')}
          </DialogDescription>
        </DialogHeader>

        <form onSubmit={handleSubmit} className="space-y-4">
          <div className="space-y-2">
            <Label htmlFor="edit-user-username">
              {t('users.users.username')}
            </Label>
            <Input
              id="edit-user-username"
              value={username}
              onChange={(e) => setUsername(e.target.value)}
              placeholder={t('users.users.usernamePlaceholder')}
              disabled={isSubmitting}
            />
          </div>

          <div className="space-y-2">
            <Label htmlFor="edit-user-password">
              {t('users.users.passwordOptional')}
            </Label>
            <Input
              id="edit-user-password"
              type="password"
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              placeholder={t('users.users.passwordPlaceholder')}
              disabled={isSubmitting}
            />
          </div>

          <DialogFooter>
            <Button
              type="button"
              variant="outline"
              onClick={() => onOpenChange(false)}
              disabled={isSubmitting}
            >
              {t('users.common.cancel')}
            </Button>
            <Button type="submit" disabled={isSubmitting}>
              {isSubmitting && (
                <Loader2 className="mr-2 h-4 w-4 animate-spin" />
              )}
              {t('users.users.saveUser')}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
