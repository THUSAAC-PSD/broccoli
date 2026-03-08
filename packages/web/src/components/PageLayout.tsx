import { Slot } from '@broccoli/web-sdk/react';

interface PageLayoutProps {
  pageId: string;
  icon?: React.ReactNode;
  title: string;
  subtitle?: string;
  contentClassName?: string;
  children: React.ReactNode;
}

export function PageLayout({
  pageId,
  icon,
  title,
  subtitle,
  contentClassName,
  children,
}: PageLayoutProps) {
  return (
    <Slot name={`${pageId}.page`} as="div" className="p-6">
      <div className="sticky top-0 z-10 bg-background -mx-6 px-6 pt-6 -mt-6 pb-4 mb-4 border-b">
        {/* Row 1: title + countdown (via slot) */}
        <Slot
          name={`${pageId}.header`}
          as="div"
          className="flex flex-col sm:flex-row items-start sm:items-center gap-4"
        >
          <div className="flex items-center gap-3 min-w-0 shrink-0">
            {icon}
            <Slot name={`${pageId}.header.title`} as="div" className="min-w-0">
              <h1 className="text-2xl font-bold tracking-tight">{title}</h1>
            </Slot>
          </div>
        </Slot>
        {/* Row 2: description */}
        {subtitle && (
          <p className="text-sm text-muted-foreground mt-3 max-h-16 overflow-y-auto">
            {subtitle}
          </p>
        )}
      </div>
      <Slot
        name={`${pageId}.content`}
        as="div"
        className={contentClassName ?? 'flex flex-col gap-4'}
      >
        {children}
      </Slot>
    </Slot>
  );
}
