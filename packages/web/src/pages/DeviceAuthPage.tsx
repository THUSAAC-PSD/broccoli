import { useApiClient } from '@broccoli/sdk/api';
import { useTranslation } from '@broccoli/sdk/i18n';
import { type FormEvent, useState } from 'react';
import { useNavigate, useSearchParams } from 'react-router';

import { Button } from '@/components/ui/button';
import {
  Card,
  CardContent,
  CardFooter,
  CardHeader,
  CardTitle,
} from '@/components/ui/card';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { useAuth } from '@/contexts/auth-context';

export function DeviceAuthPage() {
  const { t } = useTranslation();
  const { user } = useAuth();
  const navigate = useNavigate();
  const [searchParams] = useSearchParams();
  const apiClient = useApiClient();

  const [userCode, setUserCode] = useState('');
  const [error, setError] = useState('');
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [isAuthorized, setIsAuthorized] = useState(false);

  // If not logged in, redirect to login with return URL
  if (!user) {
    const returnUrl = `/auth/device${searchParams.toString() ? `?${searchParams.toString()}` : ''}`;
    navigate(`/login?redirect=${encodeURIComponent(returnUrl)}`, {
      replace: true,
    });
    return null;
  }

  // Auto-format user code input with hyphen
  const handleCodeChange = (value: string) => {
    const clean = value.toUpperCase().replace(/[^A-Z0-9]/g, '');
    if (clean.length > 4) {
      setUserCode(`${clean.slice(0, 4)}-${clean.slice(4, 8)}`);
    } else {
      setUserCode(clean);
    }
  };

  const handleSubmit = async (e: FormEvent) => {
    e.preventDefault();
    setError('');
    setIsSubmitting(true);

    try {
      const { error: apiError } = await apiClient.POST(
        '/auth/device-authorize',
        {
          body: { user_code: userCode },
        },
      );

      if (apiError) {
        const errorBody = apiError as { code?: string; message?: string };
        if (errorBody.code === 'NOT_FOUND') {
          setError(t('auth.device.codeNotFound'));
        } else if (errorBody.code === 'CONFLICT') {
          setError(t('auth.device.codeAlreadyUsed'));
        } else {
          setError(errorBody.message || t('auth.device.error'));
        }
      } else {
        setIsAuthorized(true);
      }
    } catch {
      setError(t('auth.device.error'));
    } finally {
      setIsSubmitting(false);
    }
  };

  if (isAuthorized) {
    return (
      <div className="flex min-h-screen items-smart justify-center px-4 pt-24">
        <Card className="w-full max-w-md">
          <CardHeader>
            <CardTitle className="text-2xl">
              {t('auth.device.successTitle')}
            </CardTitle>
          </CardHeader>
          <CardContent>
            <p className="text-muted-foreground">
              {t('auth.device.successMessage')}
            </p>
          </CardContent>
        </Card>
      </div>
    );
  }

  return (
    <div className="flex min-h-screen items-smart justify-center px-4 pt-24">
      <Card className="w-full max-w-md">
        <CardHeader>
          <CardTitle className="text-2xl">{t('auth.device.title')}</CardTitle>
        </CardHeader>
        <form onSubmit={handleSubmit}>
          <CardContent className="space-y-4">
            <p className="text-sm text-muted-foreground">
              {t('auth.device.instructions')}
            </p>
            {error && <p className="text-sm text-destructive">{error}</p>}
            <div className="space-y-2">
              <Label htmlFor="user-code">{t('auth.device.codeLabel')}</Label>
              <Input
                id="user-code"
                value={userCode}
                onChange={(e) => handleCodeChange(e.target.value)}
                placeholder="XXXX-XXXX"
                maxLength={9}
                className="text-center text-2xl font-mono tracking-widest"
                autoFocus
                required
              />
            </div>
          </CardContent>
          <CardFooter>
            <Button type="submit" className="w-full" disabled={isSubmitting}>
              {isSubmitting
                ? t('auth.device.authorizing')
                : t('auth.device.authorize')}
            </Button>
          </CardFooter>
        </form>
      </Card>
    </div>
  );
}
