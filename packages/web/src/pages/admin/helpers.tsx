import { Label } from '@/components/ui/label';
import { Switch } from '@/components/ui/switch';

export function SwitchField({
  id,
  label,
  checked,
  onCheckedChange,
}: {
  id: string;
  label: string;
  checked: boolean;
  onCheckedChange: (v: boolean) => void;
}) {
  return (
    <div className="flex items-center justify-between rounded-lg border p-3">
      <Label htmlFor={id} className="cursor-pointer">
        {label}
      </Label>
      <Switch id={id} checked={checked} onCheckedChange={onCheckedChange} />
    </div>
  );
}

export function getContestStatus(
  startTime: string,
  endTime: string,
  t: (key: string) => string,
): { label: string; variant: 'default' | 'secondary' | 'outline' } {
  const now = new Date();
  const start = new Date(startTime);
  const end = new Date(endTime);
  if (now < start) return { label: t('contests.upcoming'), variant: 'outline' };
  if (now <= end) return { label: t('contests.running'), variant: 'default' };
  return { label: t('contests.ended'), variant: 'secondary' };
}

export function formatDateTime(dateStr: string): string {
  return new Date(dateStr).toLocaleString(undefined, {
    year: 'numeric',
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
  });
}

export function toLocalDatetimeValue(iso: string): string {
  const d = new Date(iso);
  const offset = d.getTimezoneOffset();
  const local = new Date(d.getTime() - offset * 60000);
  return local.toISOString().slice(0, 16);
}
