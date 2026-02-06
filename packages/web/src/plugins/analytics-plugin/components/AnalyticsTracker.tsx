import { type ReactNode,useEffect } from 'react';

interface AnalyticsTrackerProps {
  children: ReactNode;
}

/**
 * AnalyticsTracker - Wraps the app to track page views and user interactions
 * This is an example of a "wrap" position plugin
 */
export function AnalyticsTracker({ children }: AnalyticsTrackerProps) {
  useEffect(() => {
    // Track page view
    console.log('[Analytics] Page view tracked');

    // Track user interactions
    const handleClick = (e: MouseEvent) => {
      const target = e.target as HTMLElement;
      if (target.tagName === 'BUTTON' || target.tagName === 'A') {
        console.log('[Analytics] Click tracked:', target.textContent);
      }
    };

    document.addEventListener('click', handleClick);

    return () => {
      document.removeEventListener('click', handleClick);
    };
  }, []);

  return <>{children}</>;
}
