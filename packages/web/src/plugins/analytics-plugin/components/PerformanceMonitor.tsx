import { Activity } from 'lucide-react';
import { SidebarMenuItem, SidebarMenuButton } from '@/components/ui/sidebar';

export function PerformanceMonitor() {
  const showMetrics = () => {
    const perfData = performance.getEntriesByType(
      'navigation',
    )[0] as PerformanceNavigationTiming;
    alert(
      `Performance Metrics:\n` +
        `Page Load: ${Math.round(perfData.duration)}ms\n` +
        `DOM Ready: ${Math.round(perfData.domContentLoadedEventEnd)}ms`,
    );
  };

  return (
    <SidebarMenuItem>
      <SidebarMenuButton onClick={showMetrics} tooltip="View performance metrics">
        <Activity />
        <span>Performance</span>
      </SidebarMenuButton>
    </SidebarMenuItem>
  );
}
