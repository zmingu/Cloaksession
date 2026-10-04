import { useEffect, useMemo, useRef, useState, type JSX, type ReactNode } from "react";
import { cn } from "../../lib/cn";
import { useScrollFade } from "../../lib/useScrollFade";

/**
 * Dense data table — the visual language borrowed from donutbrowser's
 * `ProfilesDataTable` (36px rows, sticky 32px header, one flexible name column
 * and the rest on a shared `grid-template-columns`), rebuilt on JiegeGo's own
 * design system so it adds no dependency.
 *
 * The table owns only layout: column widths, the sticky header, row chrome and
 * the scroll-fade edge. Every cell is a render function, so a screen keeps its
 * own buttons, pills and inputs and this file stays reusable.
 */

/** `flex: true` marks the single column that absorbs the remaining width. */
export interface DataTableColumn<T> {
  id: string;
  header: ReactNode;
  width?: number;
  /** Hide this column until the container is at least this wide, so
   *  low-priority columns leave first on a narrow window. */
  showFrom?: number;
  flex?: boolean;
  align?: "left" | "right";
  cell: (row: T) => ReactNode;
}

interface Props<T> {
  columns: ReadonlyArray<DataTableColumn<T>>;
  rows: ReadonlyArray<T>;
  rowKey: (row: T) => string;
  /** Stable id for a row, used for the row hover/selected states. */
  onRowClick?: (row: T) => void;
  rowClassName?: (row: T) => string | undefined;
  empty?: ReactNode;
  /** Rendered in place of the body while the first load is in flight. */
  loading?: boolean;
  /** Initial row count for the loading skeleton. */
  loadingRows?: number;
  /** Accessible label for the scroll region. */
  ariaLabel?: string;
  /** Extra className on the outer flex column. */
  className?: string;
}

const ROW_HEIGHT = 36;
const HEADER_HEIGHT = 32;
const MIN_FLEX_WIDTH = 220;

export function DataTable<T>({
  columns,
  rows,
  rowKey,
  onRowClick,
  rowClassName,
  empty,
  loading = false,
  loadingRows = 8,
  ariaLabel,
  className,
}: Props<T>): JSX.Element {
  const scrollRef = useScrollFade<HTMLDivElement>();
  const measureRef = useRef<HTMLDivElement | null>(null);
  // Quantized to 8px so a resize drag does not re-render on every pixel.
  const [width, setWidth] = useState(0);

  useEffect(() => {
    const el = measureRef.current;
    if (!el) return;
    const update = (): void => {
      setWidth(Math.round(el.clientWidth / 8) * 8);
    };
    update();
    const observer = new ResizeObserver(update);
    observer.observe(el);
    return () => observer.disconnect();
  }, []);

  // The lowest-priority columns leave first; the flexible column keeps at least
  // MIN_FLEX_WIDTH so the name never collapses to nothing.
  const visible = useMemo(() => {
    if (width === 0) return columns;
    const keep = columns.filter((column) => !column.showFrom || width >= column.showFrom);
    let fixed = 0;
    for (const column of keep) if (!column.flex) fixed += column.width ?? 120;
    const flexColumn = keep.find((column) => column.flex);
    if (!flexColumn || width - fixed >= MIN_FLEX_WIDTH) return keep;
    // Drop right-most non-flex columns until the name column fits again.
    const result = [...keep];
    while (result.length > 1) {
      let index = -1;
      for (let i = result.length - 1; i >= 0; i -= 1) {
        if (!result[i].flex) {
          index = i;
          break;
        }
      }
      if (index < 0) break;
      const candidate = result[index];
      if (candidate.width !== undefined && width - (fixed - candidate.width) >= MIN_FLEX_WIDTH) break;
      result.splice(index, 1);
      fixed -= candidate.width ?? 120;
    }
    return result;
  }, [columns, width]);

  const template = visible
    .map((column) => (column.flex ? `minmax(${MIN_FLEX_WIDTH}px, 1fr)` : `${column.width ?? 120}px`))
    .join(" ");

  return (
    <div
      ref={measureRef}
      className={cn("flex min-h-0 min-w-0 flex-1 flex-col", className)}
      data-slot="data-table"
    >
      <div
        ref={scrollRef}
        role="region"
        aria-label={ariaLabel}
        className={cn("scroll-fade min-h-0 flex-1 overflow-auto", rows.length > 0 && "pb-1")}
        style={{ ["--scroll-fade-top-offset" as string]: `${HEADER_HEIGHT}px` }}
      >
        <div
          role="table"
          className="w-full"
          style={{ minWidth: visible.reduce((sum, c) => sum + (c.flex ? MIN_FLEX_WIDTH : c.width ?? 120), 0) }}
        >
          <div
            role="row"
            className="sticky top-0 z-10 grid items-center gap-2 border-b border-white/[0.07] px-3"
            style={{
              gridTemplateColumns: template,
              height: HEADER_HEIGHT,
              background: "rgba(10,11,15,0.92)",
              backdropFilter: "blur(12px)",
            }}
          >
            {visible.map((column) => (
              <div
                key={column.id}
                role="columnheader"
                className={cn(
                  "truncate text-[10px] font-semibold uppercase tracking-wider text-slate-600",
                  column.align === "right" && "text-right",
                )}
              >
                {column.header}
              </div>
            ))}
          </div>

          {rows.length === 0 ? (
            <div className="px-3">
              {loading
                ? Array.from({ length: loadingRows }, (_, index) => (
                    <div
                      key={`skeleton-${index}`}
                      className="flex items-center gap-3 border-b border-white/[0.04] px-1"
                      style={{ height: ROW_HEIGHT }}
                    >
                      <span className="size-6 shrink-0 animate-pulse rounded-lg bg-white/[0.06]" />
                      <span
                        className="h-3 animate-pulse rounded bg-white/[0.06]"
                        style={{ width: `${30 + ((index * 17) % 40)}%` }}
                      />
                      <span className="flex-1" />
                      <span className="h-3 w-16 animate-pulse rounded bg-white/[0.06]" />
                    </div>
                  ))
                : empty}
            </div>
          ) : (
            rows.map((row) => {
              const key = rowKey(row);
              const extra = rowClassName?.(row);
              return (
                <div
                  key={key}
                  role="row"
                  onClick={onRowClick ? () => onRowClick(row) : undefined}
                  className={cn(
                    "grid items-center gap-2 border-b border-white/[0.04] px-3 transition-colors hover:bg-white/[0.025]",
                    onRowClick && "cursor-pointer",
                    extra,
                  )}
                  style={{ gridTemplateColumns: template, height: ROW_HEIGHT }}
                >
                  {visible.map((column) => (
                    <div
                      key={column.id}
                      role="cell"
                      className={cn("min-w-0", column.align === "right" && "flex justify-end")}
                    >
                      {column.cell(row)}
                    </div>
                  ))}
                </div>
              );
            })
          )}
        </div>
      </div>
    </div>
  );
}
