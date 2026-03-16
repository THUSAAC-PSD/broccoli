import { useTranslation } from '@broccoli/web-sdk/i18n';
import type { PluginDetail } from '@broccoli/web-sdk/plugin';
import {
  Badge,
  Button,
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
  Switch,
} from '@broccoli/web-sdk/ui';
import { Cpu, Globe, Loader2, RefreshCw, Server, Settings } from 'lucide-react';
import { useState } from 'react';

import { ResourceConfigDialog } from '@/components/config';

export function PluginCard({
  plugin,
  toggling,
  reloading = false,
  onToggle,
  onReload,
  onClick,
}: {
  plugin: PluginDetail;
  toggling: boolean;
  reloading?: boolean;
  onToggle: (plugin: PluginDetail, enable: boolean) => void;
  onReload?: (plugin: PluginDetail) => void;
  onClick?: (plugin: PluginDetail) => void;
}) {
  const { t } = useTranslation();
  const [configOpen, setConfigOpen] = useState(false);
  const isEnabled = plugin.status === 'Loaded';
  const hasPluginSchemas = plugin.config_schemas?.some((schema) =>
    schema.scopes.includes('plugin'),
  );

  return (
    <>
      <Card
        className={`flex flex-col${onClick ? ' cursor-pointer transition-colors hover:bg-muted/50' : ''}`}
        onClick={() => onClick?.(plugin)}
      >
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
            <div
              className="flex items-center gap-2 shrink-0"
              onClick={(e) => e.stopPropagation()}
            >
              {hasPluginSchemas && (
                <Button
                  variant="ghost"
                  size="icon"
                  className="h-8 w-8"
                  onClick={() => setConfigOpen(true)}
                  aria-label={t('plugins.configure')}
                  title={t('plugins.configure')}
                >
                  <Settings className="h-4 w-4" />
                </Button>
              )}
              {isEnabled && onReload && (
                <Button
                  variant="ghost"
                  size="icon"
                  className="h-8 w-8"
                  onClick={() => onReload(plugin)}
                  disabled={reloading || toggling}
                  aria-label={t('plugins.reload')}
                  title={t('plugins.reload')}
                >
                  {reloading ? (
                    <Loader2 className="h-4 w-4 animate-spin" />
                  ) : (
                    <RefreshCw className="h-4 w-4" />
                  )}
                </Button>
              )}
              {toggling && (
                <Loader2 className="h-4 w-4 animate-spin text-muted-foreground" />
              )}
              <Switch
                checked={isEnabled}
                disabled={toggling || reloading || plugin.status === 'Failed'}
                onCheckedChange={(checked) => onToggle(plugin, checked)}
                aria-label={
                  isEnabled ? t('plugins.disable') : t('plugins.enable')
                }
              />
            </div>
          </div>
        </CardContent>
      </Card>
      {hasPluginSchemas && (
        <ResourceConfigDialog
          scope={{ scope: 'plugin', pluginId: plugin.id }}
          resourceLabel={plugin.name}
          open={configOpen}
          onOpenChange={setConfigOpen}
        />
      )}
    </>
  );
}
