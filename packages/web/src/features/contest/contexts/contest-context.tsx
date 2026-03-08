import { useEffect } from 'react';
import { createContext, type ReactNode, use, useState } from 'react';

import { useAuth } from '@/features/auth/hooks/use-auth';

export type DashboardTab = 'problems' | 'submissions' | 'ranking';

interface ContestContextValue {
  contestId: number | null;
  contestTitle: string | null;
  activeTab: DashboardTab;
  filterProblemId: number | null;
  setActiveTab: (tab: DashboardTab) => void;
  setContest: (id: number, title: string) => void;
  clearContest: () => void;
  viewSubmissions: (problemId?: number) => void;
  setFilterProblemId: (id: number | null) => void;
}

const ContestContext = createContext<ContestContextValue | null>(null);

export function ContestProvider({ children }: { children: ReactNode }) {
  const { user } = useAuth();
  const [contestId, setContestId] = useState<number | null>(null);
  const [contestTitle, setContestTitle] = useState<string | null>(null);
  const [activeTab, setActiveTab] = useState<DashboardTab>('problems');
  const [filterProblemId, setFilterProblemId] = useState<number | null>(null);

  const setContest = (id: number, title: string) => {
    setContestId(id);
    setContestTitle(title);
    setActiveTab('problems');
    setFilterProblemId(null);
  };

  const clearContest = () => {
    setContestId(null);
    setContestTitle(null);
    setActiveTab('problems');
    setFilterProblemId(null);
  };

  // Clear contest state when user logs out
  useEffect(() => {
    if (!user) {
      clearContest();
    }
  }, [user]);

  const viewSubmissions = (problemId?: number) => {
    setFilterProblemId(problemId ?? null);
    setActiveTab('submissions');
  };

  return (
    <ContestContext
      value={{
        contestId,
        contestTitle,
        activeTab,
        filterProblemId,
        setActiveTab,
        setContest,
        clearContest,
        viewSubmissions,
        setFilterProblemId,
      }}
    >
      {children}
    </ContestContext>
  );
}

export function useContest() {
  const ctx = use(ContestContext);
  if (!ctx) throw new Error('useContest must be used within ContestProvider');
  return ctx;
}
