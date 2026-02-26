import { createContext, useContext, useState, type ReactNode } from 'react';

export type DashboardTab = 'problems' | 'submissions' | 'ranking';

interface ContestContextValue {
  contestId: number | null;
  contestTitle: string | null;
  activeTab: DashboardTab;
  setActiveTab: (tab: DashboardTab) => void;
  setContest: (id: number, title: string) => void;
  clearContest: () => void;
}

const ContestContext = createContext<ContestContextValue | null>(null);

export function ContestProvider({ children }: { children: ReactNode }) {
  const [contestId, setContestId] = useState<number | null>(null);
  const [contestTitle, setContestTitle] = useState<string | null>(null);
  const [activeTab, setActiveTab] = useState<DashboardTab>('problems');

  const setContest = (id: number, title: string) => {
    setContestId(id);
    setContestTitle(title);
    setActiveTab('problems');
  };

  const clearContest = () => {
    setContestId(null);
    setContestTitle(null);
    setActiveTab('problems');
  };

  return (
    <ContestContext.Provider
      value={{
        contestId,
        contestTitle,
        activeTab,
        setActiveTab,
        setContest,
        clearContest,
      }}
    >
      {children}
    </ContestContext.Provider>
  );
}

export function useContest() {
  const ctx = useContext(ContestContext);
  if (!ctx) throw new Error('useContest must be used within ContestProvider');
  return ctx;
}
