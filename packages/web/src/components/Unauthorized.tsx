import { useTranslation } from '@broccoli/web-sdk/i18n';
import { Card, CardContent } from '@broccoli/web-sdk/ui';
import { Shield } from 'lucide-react';

export function Unauthorized() {
  const { t } = useTranslation();
  return (
    <div className="flex items-center justify-center h-full">
      <Card className="max-w-md">
        <CardContent className="pt-6 text-center">
          <Shield className="mx-auto h-12 w-12 text-muted-foreground mb-4" />
          <p className="text-destructive text-lg font-medium">
            {t('admin.unauthorized')}
          </p>
        </CardContent>
      </Card>
    </div>
  );
}
