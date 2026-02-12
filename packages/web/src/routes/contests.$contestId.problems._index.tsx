import { useParams } from 'react-router';

import { ProblemsPage } from '@/pages/ProblemsPage';

export default function ContestProblemsIndex() {
  const { contestId } = useParams();
  return <ProblemsPage contestId={Number(contestId)} />;
}
