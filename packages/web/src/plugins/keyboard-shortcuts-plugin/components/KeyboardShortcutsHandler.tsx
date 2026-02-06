import { type ReactNode,useEffect } from 'react';

interface KeyboardShortcutsHandlerProps {
  children: ReactNode;
}

export function KeyboardShortcutsHandler({
  children,
}: KeyboardShortcutsHandlerProps) {
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      const isMac = navigator.platform.toUpperCase().indexOf('MAC') >= 0;
      const modifierKey = isMac ? e.metaKey : e.ctrlKey;

      // Ctrl/Cmd + Enter: Submit code
      if (modifierKey && e.key === 'Enter') {
        e.preventDefault();
        const submitButton = document.querySelector(
          'button[type="submit"], button:has-text("Submit")',
        ) as HTMLButtonElement;
        if (submitButton) {
          submitButton.click();
        } else {
          // Fallback: find button with text "Submit"
          const buttons = Array.from(document.querySelectorAll('button'));
          const submit = buttons.find((btn) =>
            btn.textContent?.includes('Submit'),
          );
          if (submit) {
            submit.click();
          }
        }
        console.log('[Keyboard Shortcuts] Triggered: Submit code');
        return;
      }

      // Ctrl/Cmd + /: Toggle fullscreen
      if (modifierKey && e.key === '/') {
        e.preventDefault();
        const fullscreenButton = document.querySelector(
          'button[aria-label="Toggle fullscreen"]',
        ) as HTMLButtonElement;
        if (fullscreenButton) {
          fullscreenButton.click();
        } else {
          // Fallback: find maximize/minimize button
          const buttons = Array.from(document.querySelectorAll('button'));
          const toggle = buttons.find(
            (btn) =>
              btn
                .querySelector('svg')
                ?.classList.contains('lucide-maximize-2') ||
              btn.querySelector('svg')?.classList.contains('lucide-minimize-2'),
          );
          if (toggle) {
            toggle.click();
          }
        }
        console.log('[Keyboard Shortcuts] Triggered: Toggle fullscreen');
        return;
      }

      // Ctrl/Cmd + R: Run code
      if (modifierKey && e.key === 'r') {
        e.preventDefault();
        const runButton = document.querySelector(
          'button:has-text("Run")',
        ) as HTMLButtonElement;
        if (runButton) {
          runButton.click();
        } else {
          const buttons = Array.from(document.querySelectorAll('button'));
          const run = buttons.find((btn) => btn.textContent?.includes('Run'));
          if (run) {
            run.click();
          }
        }
        console.log('[Keyboard Shortcuts] Triggered: Run code');
        return;
      }
    };

    document.addEventListener('keydown', handleKeyDown);

    return () => {
      document.removeEventListener('keydown', handleKeyDown);
    };
  }, []);

  return <>{children}</>;
}
