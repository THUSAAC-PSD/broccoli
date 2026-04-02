/**
 * Submit-gating context. Allows plugins to block the submit button.
 *
 * Plugins register "gates" (e.g., cooldown timer, submission limit) via the
 * `useSubmitGate` hook. The host app reads `useSubmitGating()` to determine
 * whether the submit button should be disabled and why.
 */
import {
  createContext,
  type ReactNode,
  use,
  useCallback,
  useEffect,
  useMemo,
  useState,
} from 'react';

export interface SubmitGate {
  id: string;
  blocked: boolean;
  reason?: string;
}

export interface SubmitGatingContextValue {
  setGate: (gate: SubmitGate) => void;
  removeGate: (id: string) => void;
  /** True if any registered gate is blocked. */
  isBlocked: boolean;
  /** Human-readable reason from the first blocking gate (if any). */
  blockReason: string | undefined;
}

const SubmitGatingContext = createContext<SubmitGatingContextValue | undefined>(
  undefined,
);

export function SubmitGatingProvider({ children }: { children: ReactNode }) {
  const [gates, setGates] = useState<Map<string, SubmitGate>>(new Map());

  const setGate = useCallback((gate: SubmitGate) => {
    setGates((prev) => {
      const existing = prev.get(gate.id);
      if (
        existing &&
        existing.blocked === gate.blocked &&
        existing.reason === gate.reason
      ) {
        return prev;
      }
      const next = new Map(prev);
      next.set(gate.id, gate);
      return next;
    });
  }, []);

  const removeGate = useCallback((id: string) => {
    setGates((prev) => {
      if (!prev.has(id)) return prev;
      const next = new Map(prev);
      next.delete(id);
      return next;
    });
  }, []);

  const isBlocked = useMemo(() => {
    for (const gate of gates.values()) {
      if (gate.blocked) return true;
    }
    return false;
  }, [gates]);

  const blockReason = useMemo(() => {
    for (const gate of gates.values()) {
      if (gate.blocked && gate.reason) return gate.reason;
    }
    return undefined;
  }, [gates]);

  const value = useMemo(
    () => ({ setGate, removeGate, isBlocked, blockReason }),
    [setGate, removeGate, isBlocked, blockReason],
  );

  return <SubmitGatingContext value={value}>{children}</SubmitGatingContext>;
}

/**
 * Read the submit-gating state. Returns `undefined` when outside a
 * `SubmitGatingProvider` (i.e., when the component is rendered in a context
 * that doesn't support gating, such as standalone code editors).
 */
export function useSubmitGating(): SubmitGatingContextValue | undefined {
  return use(SubmitGatingContext);
}

/**
 * Convenience hook for plugins to register a submit gate.
 */
export function useSubmitGate(
  id: string,
  blocked: boolean,
  reason?: string,
): void {
  const ctx = useSubmitGating();
  const setGate = ctx?.setGate;
  const removeGate = ctx?.removeGate;

  // Update gate on value change
  useEffect(() => {
    setGate?.({ id, blocked, reason });
  }, [setGate, id, blocked, reason]);

  // Remove gate on unmount only
  useEffect(() => {
    return () => removeGate?.(id);
  }, [removeGate, id]);
}
