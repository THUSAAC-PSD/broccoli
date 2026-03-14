import { useParams } from 'react-router';

import ProblemView from '@/features/problem/components/ProblemView';

export default function ContestProblemDetailPage() {
  const { problemId, contestId } = useParams();
  return (
    <ProblemView problemId={Number(problemId)} contestId={Number(contestId)} />
  );
}
