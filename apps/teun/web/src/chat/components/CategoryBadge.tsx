interface CategoryBadgeProps {
  category?: string;
}

export function CategoryBadge({ category }: CategoryBadgeProps) {
  if (category === "doorverwijzen_speciale_afhandeling") {
    return (
      <span className="inline-flex items-center gap-1 rounded-md bg-doorverwijzen-bg px-2 py-0.5 text-xs font-medium text-doorverwijzen-text">
        <span className="h-1.5 w-1.5 rounded-full bg-doorverwijzen-text/60" />
        Doorverwijzen
      </span>
    );
  }

  return (
    <span className="inline-flex items-center gap-1 rounded-md bg-standard-bg px-2 py-0.5 text-xs font-medium text-standard-text">
      <span className="h-1.5 w-1.5 rounded-full bg-standard-text/60" />
      Standaard
    </span>
  );
}
