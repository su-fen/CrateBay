interface EmptyStateProps {
  icon: React.ReactNode
  title: string
  description?: string
  code?: string
}

export function EmptyState({ icon, title, description, code }: EmptyStateProps) {
  return (
    <div className="empty-state">
      <div className="empty-icon">{icon}</div>
      <h3>{title}</h3>
      {description && <p>{description}</p>}
      {code && <code>{code}</code>}
    </div>
  )
}
