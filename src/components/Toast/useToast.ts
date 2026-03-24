import { createContext, useContext } from 'react';

/** Variant determines the visual style of the toast. */
export type ToastVariant = 'error' | 'success' | 'info';

/** A single toast notification. */
export interface Toast {
  id: number;
  message: string;
  variant: ToastVariant;
}

/** Shape of the toast context. */
export interface ToastContextValue {
  /** Show a toast notification. Auto-dismisses after the given duration (default 6s). */
  showToast: (
    message: string,
    variant?: ToastVariant,
    durationMs?: number,
  ) => void;
}

export const ToastContext = createContext<ToastContextValue | null>(null);

/** Access the toast notification system. Must be used within a ToastProvider. */
export function useToast(): ToastContextValue {
  const ctx = useContext(ToastContext);
  if (!ctx) throw new Error('useToast must be used within ToastProvider');
  return ctx;
}
