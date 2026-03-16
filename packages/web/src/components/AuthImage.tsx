import { useApiFetch } from '@broccoli/web-sdk/api';
import { useEffect, useRef, useState } from 'react';

/**
 * Image component that detects API URLs (starting with /api/) and fetches
 * them with authentication headers, rendering via blob URL. Non-API URLs
 * render as normal <img> tags.
 */
export function AuthImage({
  src,
  alt,
  className,
}: {
  src?: string;
  alt?: string;
  className?: string;
}) {
  const apiFetch = useApiFetch();
  const [blobUrl, setBlobUrl] = useState<string | null>(null);
  const [failed, setFailed] = useState(false);
  const blobUrlRef = useRef<string | null>(null);

  const needsAuth = src?.startsWith('/api/') ?? false;

  useEffect(() => {
    if (!needsAuth || !src) return;

    let revoked = false;

    apiFetch(src)
      .then((res) => {
        if (!res.ok) throw new Error();
        return res.blob();
      })
      .then((blob) => {
        if (revoked) return;
        const url = URL.createObjectURL(blob);
        blobUrlRef.current = url;
        setBlobUrl(url);
      })
      .catch(() => {
        if (!revoked) setFailed(true);
      });

    return () => {
      revoked = true;
      if (blobUrlRef.current) {
        URL.revokeObjectURL(blobUrlRef.current);
        blobUrlRef.current = null;
      }
    };
  }, [src, needsAuth, apiFetch]);

  if (!needsAuth) {
    return <img src={src} alt={alt} className={className} />;
  }

  if (!blobUrl) {
    // Show broken image on failure so users can report it to admins
    return failed ? <img alt={alt} className={className} /> : null;
  }

  return <img src={blobUrl} alt={alt} className={className} />;
}
