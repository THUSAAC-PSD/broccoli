import { Slot } from '@broccoli/web-sdk/slot';

import { Markdown } from '@/components/Markdown';

interface PageLayoutProps {
  pageId: string;
  icon?: React.ReactNode;
  title: string;
  subtitle?: string;
  actions?: React.ReactNode;
  contentClassName?: string;
  children: React.ReactNode;
}

export function PageLayout({
  pageId,
  icon,
  title,
  subtitle,
  actions,
  contentClassName,
  children,
}: PageLayoutProps) {
  return (
    <Slot name={`${pageId}.page`} as="div" className="p-6">
      <div className="sticky top-0 z-10 bg-background -mx-6 px-6 pt-6 -mt-6 pb-4 mb-4 border-b">
        <Slot
          name={`${pageId}.header`}
          as="div"
          className="flex items-center gap-4"
        >
          <div className="flex items-center gap-3 min-w-0">
            {icon}
            <Slot name={`${pageId}.header.title`} as="div" className="min-w-0">
              <h1 className="text-2xl font-bold tracking-tight">{title}</h1>
            </Slot>
          </div>
          {actions && <div className="ml-auto shrink-0">{actions}</div>}
        </Slot>
        {subtitle && (
          <Markdown className="text-sm text-muted-foreground mt-3 max-h-24 overflow-y-auto">
            {subtitle}
          </Markdown>
        )}
      </div>
      <Slot
        name={`${pageId}.content`}
        as="div"
        className={contentClassName ?? 'flex flex-col gap-4'}
      >
        {children}
        <Slot
          name={`${pageId}.content.sidebar`}
          as="div"
          className="space-y-4"
        />
      </Slot>
    </Slot>
  );
}
