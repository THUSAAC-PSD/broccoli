import { useParams } from 'react-router';

import { ProblemsPage } from '@/features/problem/components/ProblemsPage';

export default function ContestProblemListPage() {
  const { contestId } = useParams();
  return <ProblemsPage contestId={Number(contestId)} />;
}
