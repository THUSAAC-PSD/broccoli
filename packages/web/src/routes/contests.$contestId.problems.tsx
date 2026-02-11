import { useParams } from 'react-router';

import { ProblemsPage } from '@/pages/ProblemsPage';

export default function ContestProblems() {
  const { contestId } = useParams();
  return <ProblemsPage contestId={Number(contestId)} />;
}
