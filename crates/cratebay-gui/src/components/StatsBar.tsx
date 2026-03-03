interface StatsBarProps {
  label: string
  value: number
  max?: number
  suffix?: string
  color?: string
}

export function StatsBar({ label, value, max = 100, suffix = "%", color }: StatsBarProps) {
  const pct = max > 0 ? Math.min((value / max) * 100, 100) : 0
  const barColor = color || (pct > 80 ? "var(--red)" : pct > 50 ? "var(--cyan)" : "var(--green)")
  const displayValue = suffix === "%" ? value.toFixed(1) : value < 10 ? value.toFixed(1) : Math.round(value).toString()

  return (
    <div className="stats-bar-item">
      <div className="stats-bar-label">{label}</div>
      <div className="stats-bar-track">
        <div className="stats-bar-fill" style={{ width: `${pct}%`, background: barColor }} />
      </div>
      <div className="stats-bar-value">{displayValue}{suffix}</div>
    </div>
  )
}

interface MiniStatsProps {
  items: { label: string; value: number; max?: number; suffix?: string }[]
}

export function MiniStats({ items }: MiniStatsProps) {
  return (
    <div className="stats-bar-row">
      {items.map(item => (
        <StatsBar key={item.label} {...item} />
      ))}
    </div>
  )
}
