import { useEffect, useRef } from "react";
import type { ReactNode } from "react";
import { ChevronDown } from "lucide-react";
import { focusableControls } from "./modal";

/** Full-width group label inside a menu, matching the grouped menus recorded on FL Studio. */
export function MenuGroup({ label }: { label: string }) {
  return (
    <div className="menu-group" role="presentation">
      {label}
    </div>
  );
}

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
        e.stopPropagation();
        const menu = ref.current;
        if (!menu) return;
        if (
          ["ArrowDown", "ArrowUp", "Home", "End"].includes(e.key) &&
          !(e.target as HTMLElement).closest(
            'input,select,textarea,[role="slider"]',
          )
        ) {
          e.preventDefault();
          menu.open = true;
          const controls = focusableControls(
            menu.querySelector<HTMLElement>(".studio-menu-content")!,
          );
          if (!controls.length) return;
          const index = controls.indexOf(document.activeElement as HTMLElement);
          const next =
            e.key === "Home"
              ? 0
              : e.key === "End"
                ? controls.length - 1
                : e.key === "ArrowDown"
                  ? (index + 1) % controls.length
                  : (index <= 0 ? controls.length : index) - 1;
          controls[next]?.focus();
        }
        if (e.key === "Escape" && ref.current?.open) {
          e.preventDefault();
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
          if ((e.target as HTMLElement).closest("button") && ref.current) {
            ref.current.open = false;
            ref.current.querySelector("summary")?.focus();
          }
        }}
      >
        {children}
      </div>
    </details>
  );
}
