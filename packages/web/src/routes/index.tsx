import { createFileRoute } from '@tanstack/react-router';

import { ProblemPage } from '@/pages/ProblemPage';

export const Route = createFileRoute('/')({
  component: ProblemPage,
});
