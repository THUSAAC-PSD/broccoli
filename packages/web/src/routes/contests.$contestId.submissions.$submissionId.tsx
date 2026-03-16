import { Navigate, useParams } from 'react-router';

import { SubmissionDetailView } from '@/features/submission/components/SubmissionDetailView';

export default function SubmissionDetail() {
  const { submissionId, contestId } = useParams();
  const sid = Number(submissionId);
  const cid = Number(contestId);

  if (isNaN(sid) || isNaN(cid)) {
    return <Navigate to="/" replace />;
  }

  return <SubmissionDetailView submissionId={sid} contestId={cid} />;
}
