const TELEMETRY_BASE = '/api/v1/telemetry';

export function reportError(error: {
  message: string;
  stack?: string;
  url?: string;
  requestId?: string;
}) {
  const body = JSON.stringify({
    message: error.message,
    stack: error.stack,
    url: error.url ?? window.location.href,
    request_id: error.requestId,
  });

  if (navigator.sendBeacon) {
    navigator.sendBeacon(
      `${TELEMETRY_BASE}/errors`,
      new Blob([body], { type: 'application/json' }),
    );
  } else {
    fetch(`${TELEMETRY_BASE}/errors`, {
      method: 'POST',
      body,
      headers: { 'Content-Type': 'application/json' },
      keepalive: true,
    }).catch(() => {});
  }
}

export function reportVitals() {
  import('web-vitals').then(({ onCLS, onINP, onLCP, onFCP, onTTFB }) => {
    const vitals: Array<{ name: string; value: number; url?: string }> = [];

    const flush = () => {
      if (vitals.length === 0) return;
      const body = JSON.stringify({ vitals: [...vitals] });
      vitals.length = 0;

      if (navigator.sendBeacon) {
        navigator.sendBeacon(
          `${TELEMETRY_BASE}/vitals`,
          new Blob([body], { type: 'application/json' }),
        );
      }
    };

    const collect = (metric: { name: string; value: number }) => {
      vitals.push({
        name: metric.name,
        value: metric.value,
        url: window.location.href,
      });
    };

    onCLS(collect);
    onINP(collect);
    onLCP(collect);
    onFCP(collect);
    onTTFB(collect);

    document.addEventListener('visibilitychange', () => {
      if (document.visibilityState === 'hidden') flush();
    });
  });
}
