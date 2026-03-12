import { useParams } from 'react-router';

import ProblemView from '@/features/problem/components/ProblemView';

export default function ProblemDetailPage() {
  const { problemId } = useParams();
  return <ProblemView problemId={Number(problemId)} />;
}
