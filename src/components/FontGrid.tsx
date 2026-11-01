import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { keepPreviousData, useInfiniteQuery, useQuery } from "@tanstack/react-query";
import { useVirtualizer } from "@tanstack/react-virtual";
import { SearchX, WifiOff } from "lucide-react";
import { api, errorText } from "@/lib/api";
import type { FontQuery } from "@/lib/types";
import { useUi } from "@/store/ui";
import { FontCard } from "./FontCard";
import { Button } from "./ui/button";

const PAGE = 60;
const MIN_CARD = 300;
const GAP = 10;

export function EmptyState({ icon: Icon, title, detail, action }: {
  icon: typeof SearchX; title: string; detail?: string; action?: ReactNode;
}) {
  return (
    <div className="anim-rise flex flex-col items-center justify-center py-20 text-center">
      <Icon className="size-6 text-faint" strokeWidth={1.5} />
      <div className="mt-3 text-sm font-medium">{title}</div>
      {detail && <div className="mt-1 max-w-80 text-[13px] leading-relaxed text-soft">{detail}</div>}
      {action && <div className="mt-4">{action}</div>}
    </div>
  );
}

export function GridSkeleton() {
  return (
    <div className="grid gap-2.5" style={{ gridTemplateColumns: `repeat(auto-fill, minmax(${MIN_CARD}px, 1fr))` }}>
      {Array.from({ length: 9 }, (_, i) => (
        <div key={i} className="h-[148px] rounded-[10px] border border-line bg-panel p-3.5">
          <div className="skeleton h-3 w-24" /><div className="skeleton mt-8 h-8 w-3/4" />
        </div>
      ))}
    </div>
  );
}

export function FontGrid({ query, header, empty }: { query: FontQuery; header?: ReactNode; empty?: ReactNode }) {
  const search = useUi((s) => s.search);
  const previewText = useUi((s) => s.previewText);
  const size = useUi((s) => s.previewSize);
  const selection = useUi((s) => s.selection);
  const settings = useQuery({ queryKey: ["settings"], queryFn: api.getSettings });
  const text = previewText || settings.data?.preview_text || "TYPEVAULT Aa";
  const turkishFirst = settings.data?.turkish_priority === "1" && !query.sort;
  const effective = useMemo(() => ({ ...query, search, turkishFirst }), [query, search, turkishFirst]);

  const list = useInfiniteQuery({
    queryKey: ["fonts", effective],
    queryFn: ({ pageParam }) => api.listFonts({ ...effective, offset: pageParam, limit: PAGE }),
    initialPageParam: 0,
    getNextPageParam: (last, pages) => {
      const loaded = pages.reduce((n, p) => n + p.items.length, 0);
      return loaded < last.total ? loaded : undefined;
    },
    placeholderData: keepPreviousData,
  });

  const scroller = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(900);
  useEffect(() => {
    const el = scroller.current;
    if (!el) return;
    const observer = new ResizeObserver(([entry]) => setWidth(entry.contentRect.width));
    observer.observe(el);
    return () => observer.disconnect();
  }, []);

  const inner = Math.max(width - 32, 320);
  const columns = Math.max(1, Math.floor((inner + GAP) / (MIN_CARD + GAP)));
  const rowHeight = Math.round(size * 1.5) + 86;
  const items = useMemo(() => list.data?.pages.flatMap((p) => p.items) ?? [], [list.data]);
  const rowCount = Math.ceil(items.length / columns);

  const headerRef = useRef<HTMLDivElement>(null);
  const [headerHeight, setHeaderHeight] = useState(0);
  useEffect(() => {
    const el = headerRef.current;
    if (!el) return;
    const observer = new ResizeObserver(([entry]) => setHeaderHeight(entry.contentRect.height));
    observer.observe(el);
    return () => observer.disconnect();
  }, []);

  const virtual = useVirtualizer({
    count: rowCount,
    getScrollElement: () => scroller.current,
    estimateSize: () => rowHeight + GAP,
    overscan: 3,
    scrollMargin: headerHeight,
  });
  useEffect(() => virtual.measure(), [rowHeight, columns]);

  const rows = virtual.getVirtualItems();
  const lastRow = rows[rows.length - 1]?.index ?? 0;
  useEffect(() => {
    if (lastRow >= rowCount - 3 && list.hasNextPage && !list.isFetchingNextPage) list.fetchNextPage();
  }, [lastRow, rowCount, list.hasNextPage, list.isFetchingNextPage]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const tag = (e.target as HTMLElement).tagName;
      if (e.metaKey && e.key.toLowerCase() === "a" && tag !== "INPUT" && tag !== "TEXTAREA") {
        e.preventDefault();
        useUi.getState().selectMany(items.map((f) => f.id));
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [items]);

  return (
    <div ref={scroller} className="min-h-0 flex-1 overflow-y-auto overflow-x-hidden">
      <div ref={headerRef}>{header}</div>
      <div className="px-4 pb-20">
        {list.isPending ? (
          <GridSkeleton />
        ) : list.isError ? (
          <EmptyState
            icon={WifiOff}
            title="Fontlar yüklenemedi"
            detail={errorText(list.error)}
            action={<Button onClick={() => list.refetch()}>Yeniden dene</Button>}
          />
        ) : items.length === 0 ? (
          empty ?? <EmptyState icon={SearchX} title="Sonuç bulunamadı" detail="Aramayı veya filtreleri değiştirmeyi deneyin." />
        ) : (
          <div className="relative" style={{ height: virtual.getTotalSize() }}>
            {rows.map((row) => (
              <div
                key={row.key}
                className="absolute inset-x-0 grid gap-2.5"
                style={{
                  height: rowHeight,
                  transform: `translateY(${row.start - headerHeight}px)`,
                  gridTemplateColumns: `repeat(${columns}, minmax(0, 1fr))`,
                }}
              >
                {items.slice(row.index * columns, (row.index + 1) * columns).map((font) => (
                  <FontCard key={font.id} font={font} text={text} size={size} selected={selection.has(font.id)} selecting={selection.size > 0} />
                ))}
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
