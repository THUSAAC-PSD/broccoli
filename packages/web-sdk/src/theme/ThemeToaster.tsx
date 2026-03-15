import { useTheme } from '@/theme/use-theme';
import { Toaster, type ToasterProps } from '@/ui/sonner';

export function ThemeToaster(props: ToasterProps) {
  const { theme } = useTheme();

  return <Toaster {...props} theme={theme} />;
}
