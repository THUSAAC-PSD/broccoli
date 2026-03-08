import type { PluginDetailResponse } from '@broccoli/web-sdk';
import { useTranslation } from '@broccoli/web-sdk/i18n';
import { Cpu, Globe, Loader2, Server } from 'lucide-react';

import { Badge } from '@/components/ui/badge';
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from '@/components/ui/card';
import { Switch } from '@/components/ui/switch';

export function PluginCard({
  plugin,
  toggling,
  onToggle,
}: {
  plugin: PluginDetailResponse;
  toggling: boolean;
  onToggle: (plugin: PluginDetailResponse, enable: boolean) => void;
}) {
  const { t } = useTranslation();
  const isEnabled = plugin.status === 'Loaded';

  return (
    <Card className="flex flex-col">
      <CardHeader>
        <CardTitle className="text-base flex items-center gap-2">
          <span className="truncate">{plugin.name}</span>
        </CardTitle>
        <CardDescription className="text-xs font-mono">
          {plugin.id}
          {plugin.version && (
            <span className="ml-2 text-muted-foreground">
              v{plugin.version}
            </span>
          )}
        </CardDescription>
      </CardHeader>

      <CardContent className="flex-1">
        {plugin.description && (
          <p className="text-sm text-muted-foreground mb-3">
            {plugin.description}
          </p>
        )}
        <div className="flex items-end justify-between gap-4">
          <div className="flex flex-wrap gap-2">
            {plugin.has_server && (
              <Badge variant="outline" className="gap-1 text-xs">
                <Server className="h-3 w-3" />
                {t('plugins.component.server')}
              </Badge>
            )}
            {plugin.has_web && (
              <Badge variant="outline" className="gap-1 text-xs">
                <Globe className="h-3 w-3" />
                {t('plugins.component.web')}
              </Badge>
            )}
            {plugin.has_worker && (
              <Badge variant="outline" className="gap-1 text-xs">
                <Cpu className="h-3 w-3" />
                {t('plugins.component.worker')}
              </Badge>
            )}
          </div>
          <div className="flex items-center gap-2 shrink-0">
            {toggling && (
              <Loader2 className="h-4 w-4 animate-spin text-muted-foreground" />
            )}
            <Switch
              checked={isEnabled}
              disabled={toggling || plugin.status === 'Failed'}
              onCheckedChange={(checked) => onToggle(plugin, checked)}
              aria-label={
                isEnabled ? t('plugins.disable') : t('plugins.enable')
              }
            />
          </div>
        </div>
      </CardContent>
    </Card>
  );
}
