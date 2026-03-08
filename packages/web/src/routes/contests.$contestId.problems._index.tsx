import { useParams } from 'react-router';

import { AdminProblemListView } from '@/features/admin/components/AdminProblemListView';

export default function ContestProblemListPage() {
  const { contestId } = useParams();
  return <AdminProblemListView contestId={Number(contestId)} />;
}
