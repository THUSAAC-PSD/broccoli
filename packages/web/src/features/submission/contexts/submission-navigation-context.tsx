import { type ReactNode, useState } from 'react';
import { useLocation, useNavigationType } from 'react-router';

import { SubmissionNavigationContext } from '@/features/submission/hooks/use-submission-navigation';
import {
  type NavigationHistory,
  recordNavigation,
} from '@/features/submission/utils/navigation';

export function SubmissionNavigationProvider({
  children,
}: {
  children: ReactNode;
}) {
  const { key } = useLocation();
  const action = useNavigationType();
  const [history, setHistory] = useState<NavigationHistory>({
    keys: [key],
    index: 0,
  });
  const current = recordNavigation(history, key, action);
  if (current !== history) setHistory(current);

  return (
    <SubmissionNavigationContext value={current.keys[current.index - 1]}>
      {children}
    </SubmissionNavigationContext>
  );
}
