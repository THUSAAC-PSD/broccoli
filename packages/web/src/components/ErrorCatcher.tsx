import {
  AlertTriangle,
  Ban,
  FileQuestion,
  ServerCrash,
  Home,
  Construction,
  Lock,
  FileWarning,
  Timer,
  CloudOff,
  Activity,
  Coffee,
  type LucideIcon,
} from 'lucide-react';
import { Button } from '@/components/ui/button';
import { useTranslation } from '@broccoli/sdk/i18n';

interface ErrorPageProps {
  code: number | string;
  message?: string;
  onRetry?: () => void;
  onBack?: () => void;
}

export function ErrorCatcher({ code }: ErrorPageProps) {
  const { t } = useTranslation();

  const Iconmap: Record<string, LucideIcon> = {
    '400': FileWarning,
    '401': Lock,
    '403': Ban,
    '404': FileQuestion,
    '408': Timer,
    '418': Coffee,
    '429': Activity,
    '500': ServerCrash,
    '501': Construction,
    '502': CloudOff,
    '503': ServerCrash,
    '504': Timer,
    default: AlertTriangle,
  };
  const Icon = Iconmap[String(code)] || Iconmap['default'];

  const getErrorContent = (code: string | number) => {
    return {
      title: t(`error.title.${code}`),
      desc: t(`error.desc.${code}`),
    };
  };

  const { title, desc } = getErrorContent(code);

  return (
    <div className="flex flex-col items-center justify-center min-h-[60vh] px-4 text-center space-y-6">
      <div className="p-6 rounded-full bg-muted">
        <Icon className="w-12 h-12 text-muted-foreground" />
      </div>

      <div className="space-y-2">
        <h1 className="text-3xl font-bold tracking-tighter sm:text-4xl">
          {title}
        </h1>
        <p className="text-gray-500 md:text-xl/relaxed dark:text-gray-400 max-w-[600px]">
          {desc}
        </p>
      </div>
      <div className="mt-8 flex justify-center gap-4">
        <Button onClick={() => (window.location.href = '/')} variant="default">
          <Home className="mr-2 h-4 w-4" />
          {t('error.backToHome')}
        </Button>
      </div>
    </div>
  );
}
