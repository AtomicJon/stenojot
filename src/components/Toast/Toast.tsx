import { createContext, useCallback, useContext, useState } from "react";
import type { ReactNode } from "react";
import s from "./Toast.module.scss";

/** Variant determines the visual style of the toast. */
type ToastVariant = "error" | "success" | "info";

/** A single toast notification. */
interface Toast {
  id: number;
  message: string;
  variant: ToastVariant;
}

/** Shape of the toast context. */
interface ToastContextValue {
  /** Show a toast notification. Auto-dismisses after the given duration (default 6s). */
  showToast: (message: string, variant?: ToastVariant, durationMs?: number) => void;
}

const ToastContext = createContext<ToastContextValue | null>(null);

/** Access the toast notification system. Must be used within a ToastProvider. */
export function useToast(): ToastContextValue {
  const ctx = useContext(ToastContext);
  if (!ctx) throw new Error("useToast must be used within ToastProvider");
  return ctx;
}

let nextId = 0;

/** Props for the ToastProvider component. */
interface ToastProviderProps {
  children: ReactNode;
}

/** Provides a toast notification system to all descendant components. */
export function ToastProvider({ children }: ToastProviderProps) {
  const [toasts, setToasts] = useState<Toast[]>([]);

  const dismiss = useCallback((id: number) => {
    setToasts((prev) => prev.filter((t) => t.id !== id));
  }, []);

  const showToast = useCallback(
    (message: string, variant: ToastVariant = "info", durationMs = 6000) => {
      const id = nextId++;
      setToasts((prev) => [...prev, { id, message, variant }]);
      setTimeout(() => dismiss(id), durationMs);
    },
    [dismiss]
  );

  const variantClass = (variant: ToastVariant) => {
    switch (variant) {
      case "error":
        return s.toastError;
      case "success":
        return s.toastSuccess;
      case "info":
        return s.toastInfo;
    }
  };

  return (
    <ToastContext.Provider value={{ showToast }}>
      {children}
      {toasts.length > 0 && (
        <div className={s.overlay}>
          {toasts.map((toast) => (
            <div
              key={toast.id}
              className={`${s.toast} ${variantClass(toast.variant)}`}
            >
              <span className={s.message}>{toast.message}</span>
              <button className={s.dismiss} onClick={() => dismiss(toast.id)}>
                x
              </button>
            </div>
          ))}
        </div>
      )}
    </ToastContext.Provider>
  );
}
