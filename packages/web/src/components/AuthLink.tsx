import { useApiFetch } from '@broccoli/web-sdk/api';
import { type ReactNode, useCallback } from 'react';

/**
 * Link component that detects API URLs (starting with /api/) and handles
 * clicks by fetching with authentication headers, then triggering a browser
 * download from the blob. Non-API URLs render as normal <a> tags.
 */
export function AuthLink({
  href,
  children,
  className,
}: {
  href?: string;
  children?: ReactNode;
  className?: string;
}) {
  const apiFetch = useApiFetch();
  const needsAuth = href?.startsWith('/api/') ?? false;

  const handleClick = useCallback(
    async (e: React.MouseEvent<HTMLAnchorElement>) => {
      if (!needsAuth || !href) return;
      e.preventDefault();

      try {
        const res = await apiFetch(href);
        if (!res.ok) throw new Error(`Download failed (${res.status})`);

        const blob = await res.blob();
        const url = URL.createObjectURL(blob);

        // Extract filename from Content-Disposition header, or fall back to URL path
        let filename = 'download';
        const disposition = res.headers.get('content-disposition');
        if (disposition) {
          const match = /filename\*?=(?:UTF-8''|"?)([^";]+)/.exec(disposition);
          if (match) filename = decodeURIComponent(match[1].replace(/"/g, ''));
        } else {
          const segments = href.split('/');
          const last = segments[segments.length - 1];
          if (last) filename = last;
        }

        const a = document.createElement('a');
        a.href = url;
        a.download = filename;
        a.style.display = 'none';
        document.body.appendChild(a);
        a.click();
        document.body.removeChild(a);
        setTimeout(() => URL.revokeObjectURL(url), 1000);
      } catch {
        // Fall back to opening the URL directly
        window.open(href, '_blank');
      }
    },
    [href, needsAuth, apiFetch],
  );

  if (!needsAuth) {
    return (
      <a
        href={href}
        className={className}
        target="_blank"
        rel="noopener noreferrer"
      >
        {children}
      </a>
    );
  }

  return (
    <a href={href} className={className} onClick={handleClick}>
      {children}
    </a>
  );
}
