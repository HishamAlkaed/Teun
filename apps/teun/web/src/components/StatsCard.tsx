interface StatsCardProps {
  label: string;
  value: string | number;
  subtext?: string;
  color?: "default" | "green" | "yellow" | "red";
}

export function StatsCard({ label, value, subtext, color = "default" }: StatsCardProps) {
  const colorClasses = {
    default: "text-text-primary",
    green: "text-emerald-600",
    yellow: "text-amber-600",
    red: "text-red-600",
  };

  return (
    <div className="bg-surface border border-border rounded-lg p-4">
      <div className="text-xs text-text-tertiary mb-1">{label}</div>
      <div className={`text-2xl font-semibold ${colorClasses[color]}`}>{value}</div>
      {subtext && <div className="text-xs text-text-tertiary mt-1">{subtext}</div>}
    </div>
  );
}
