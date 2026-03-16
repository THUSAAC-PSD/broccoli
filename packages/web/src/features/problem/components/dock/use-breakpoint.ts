import { useEffect, useState } from 'react';

type Breakpoint = 'mobile' | 'tablet' | 'desktop';

const DESKTOP_MIN = 1024;
const TABLET_MIN = 768;

function getBreakpoint(width: number): Breakpoint {
  if (width >= DESKTOP_MIN) return 'desktop';
  if (width >= TABLET_MIN) return 'tablet';
  return 'mobile';
}

export function useBreakpoint(): Breakpoint {
  const [breakpoint, setBreakpoint] = useState<Breakpoint>(() =>
    typeof window !== 'undefined'
      ? getBreakpoint(window.innerWidth)
      : 'desktop',
  );

  useEffect(() => {
    const desktopMql = window.matchMedia(`(min-width: ${DESKTOP_MIN}px)`);
    const tabletMql = window.matchMedia(`(min-width: ${TABLET_MIN}px)`);

    const update = () => setBreakpoint(getBreakpoint(window.innerWidth));

    desktopMql.addEventListener('change', update);
    tabletMql.addEventListener('change', update);
    update();

    return () => {
      desktopMql.removeEventListener('change', update);
      tabletMql.removeEventListener('change', update);
    };
  }, []);

  return breakpoint;
}
