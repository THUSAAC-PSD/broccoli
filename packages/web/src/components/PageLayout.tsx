import { Slot } from '@broccoli/web-sdk/react';

interface PageLayoutProps {
  pageId: string;
  icon?: React.ReactNode;
  title: string;
  subtitle?: string;
  children: React.ReactNode;
}

export function PageLayout({
  pageId,
  icon,
  title,
  subtitle,
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
        <Slot name={`${pageId}.header.title`} as="div">
          <h1 className="text-2xl font-bold tracking-tight">{title}</h1>
          {subtitle && <p className="text-muted-foreground">{subtitle}</p>}
        </Slot>
      </Slot>
      <Slot name={`${pageId}.content`} as="div" className="flex flex-col gap-4">
        {children}
      </Slot>
    </Slot>
  );
}
