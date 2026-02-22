import {
  createContext,
  useContext,
  useState,
  useCallback,
  useRef,
  useEffect,
} from "react";
import type { ReactNode, JSX } from "react";

type ToastVariant = "success" | "error" | "warning" | "info";

interface ToastEntry {
  id: number;
  message: string;
  variant: ToastVariant;
  leaving: boolean;
}

interface ToastContextValue {
  showToast: (message: string, variant?: ToastVariant) => void;
}

const ToastContext = createContext<ToastContextValue | null>(null);

export function useToast(): ToastContextValue {
  const ctx = useContext(ToastContext);
  if (!ctx) throw new Error("useToast must be used within ToastProvider");
  return ctx;
}

function ToastItem({ entry, onDone }: { entry: ToastEntry; onDone: (id: number) => void }): JSX.Element {
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    if (entry.leaving) {
      timerRef.current = setTimeout(() => onDone(entry.id), 300);
    }
    return () => { if (timerRef.current) clearTimeout(timerRef.current); };
  }, [entry.leaving, entry.id, onDone]);

  const variantClass =
    entry.variant === "error" ? "toast--error"
    : entry.variant === "warning" ? "toast--warning"
    : entry.variant === "info" ? "toast--info"
    : "toast--success";

  return (
    <div className={`toast ${variantClass}${entry.leaving ? " toast--leaving" : ""}`}>
      <span className="toast-message">{entry.message}</span>
    </div>
  );
}

export function ToastProvider({ children }: { children: ReactNode }): JSX.Element {
  const [toasts, setToasts] = useState<ToastEntry[]>([]);
  const nextId = useRef(0);
  const timers = useRef<Map<number, ReturnType<typeof setTimeout>>>(new Map());

  useEffect(() => {
    return () => {
      for (const t of timers.current.values()) clearTimeout(t);
    };
  }, []);

  const showToast = useCallback((message: string, variant: ToastVariant = "success") => {
    const id = nextId.current++;
    setToasts((prev) => [...prev, { id, message, variant, leaving: false }]);

    const leaveTimer = setTimeout(() => {
      setToasts((prev) => prev.map((t) => (t.id === id ? { ...t, leaving: true } : t)));
      timers.current.delete(id);
    }, 2500);

    timers.current.set(id, leaveTimer);
  }, []);

  const handleDone = useCallback((id: number) => {
    setToasts((prev) => prev.filter((t) => t.id !== id));
  }, []);

  return (
    <ToastContext.Provider value={{ showToast }}>
      {children}
      <div className="toast-container" aria-live="polite">
        {toasts.map((entry) => (
          <ToastItem key={entry.id} entry={entry} onDone={handleDone} />
        ))}
      </div>
    </ToastContext.Provider>
  );
}
