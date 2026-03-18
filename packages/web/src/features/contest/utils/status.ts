export function getContestStatus(
  startTime: string,
  endTime: string,
  t: (key: string) => string,
): { label: string; variant: 'default' | 'secondary' | 'outline-solid' } {
  const now = new Date();
  const start = new Date(startTime);
  const end = new Date(endTime);

  if (now < start) return { label: t('contests.upcoming'), variant: 'outline' };
  if (now <= end) return { label: t('contests.running'), variant: 'default' };
  return { label: t('contests.ended'), variant: 'secondary' };
}
