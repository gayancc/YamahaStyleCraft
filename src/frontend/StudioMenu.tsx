import { useEffect, useRef } from "react";
import type { ReactNode } from "react";
import { ChevronDown } from "lucide-react";

/** Native disclosure keyboard behavior, with outside-click and Escape dismissal. */
export function StudioMenu({
  label,
  children,
  className = "",
}: {
  label: string;
  children: ReactNode;
  className?: string;
}) {
  const ref = useRef<HTMLDetailsElement>(null);
  useEffect(() => {
    const outside = (e: PointerEvent) => {
      if (!ref.current?.contains(e.target as Node) && ref.current)
        ref.current.open = false;
    };
    document.addEventListener("pointerdown", outside);
    return () => document.removeEventListener("pointerdown", outside);
  }, []);
  return (
    <details
      ref={ref}
      className={`studio-menu ${className}`}
      onKeyDown={(e) => {
        if (e.key === "Escape" && ref.current?.open) {
          e.stopPropagation();
          ref.current.open = false;
          ref.current.querySelector("summary")?.focus();
        }
      }}
    >
      <summary>
        {label}
        <ChevronDown size={12} />
      </summary>
      <div
        className="studio-menu-content"
        onClick={(e) => {
          if ((e.target as HTMLElement).closest("button") && ref.current)
            ref.current.open = false;
        }}
      >
        {children}
      </div>
    </details>
  );
}
