import { Slot } from '@broccoli/sdk/react';

interface PageLayoutProps {
  pageId: string;
  icon?: React.ReactNode;
  title: string;
  subtitle?: string;
  actions?: React.ReactNode;
  children: React.ReactNode;
}

export function PageLayout({
  pageId,
  icon,
  title,
  subtitle,
  actions,
  children,
}: PageLayoutProps) {
  return (
    <Slot name={`${pageId}.page`} as="div" className="p-6">
      <Slot
        name={`${pageId}.header`}
        as="div"
        className="mb-4 flex items-center gap-3"
      >
        {icon}
        <Slot name={`${pageId}.header.title`} as="div" className="flex-1">
          <h1 className="text-2xl font-bold tracking-tight">{title}</h1>
          {subtitle && <p className="text-muted-foreground">{subtitle}</p>}
        </Slot>
        {actions}
      </Slot>
      <Slot name={`${pageId}.content`} as="div" className="flex flex-col gap-4">
        {children}
      </Slot>
    </Slot>
  );
}
