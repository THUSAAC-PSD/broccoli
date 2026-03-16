import { useTranslation } from '@broccoli/web-sdk/i18n';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@broccoli/web-sdk/ui';
import { useEffect, useState } from 'react';

import { CodeEditorPanel } from '../CodeEditorPanel';
import { ProblemStatementPanel } from '../ProblemStatementPanel';
import { SubmissionsPanel } from '../SubmissionsPanel';

interface ProblemMobileLayoutProps {
  problemId: number;
}

export function ProblemMobileLayout({ problemId }: ProblemMobileLayoutProps) {
  const { t } = useTranslation();
  const [activeTab, setActiveTab] = useState('statement');

  // Reset to statement tab when navigating between problems
  useEffect(() => {
    setActiveTab('statement');
  }, [problemId]);

  return (
    <Tabs
      value={activeTab}
      onValueChange={setActiveTab}
      className="flex-1 flex flex-col min-h-0"
    >
      <TabsList className="flex-shrink-0 w-full justify-start rounded-none border-b bg-transparent px-2">
        <TabsTrigger value="statement" className="text-xs">
          {t('problem.description', { defaultValue: 'Statement' })}
        </TabsTrigger>
        <TabsTrigger value="editor" className="text-xs">
          {t('editor.title', { defaultValue: 'Editor' })}
        </TabsTrigger>
        <TabsTrigger value="submissions" className="text-xs">
          {t('result.title', { defaultValue: 'Submissions' })}
        </TabsTrigger>
      </TabsList>

      <TabsContent value="statement" className="flex-1 min-h-0 mt-0">
        <ProblemStatementPanel />
      </TabsContent>

      <TabsContent value="editor" className="flex-1 min-h-0 mt-0">
        <div className="h-full flex flex-col">
          <CodeEditorPanel />
        </div>
      </TabsContent>

      <TabsContent value="submissions" className="flex-1 min-h-0 mt-0">
        <div className="h-full">
          <SubmissionsPanel />
        </div>
      </TabsContent>
    </Tabs>
  );
}
