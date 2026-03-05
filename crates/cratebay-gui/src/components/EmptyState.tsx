interface EmptyStateProps {
  icon: React.ReactNode
  title: string
  description?: string
  code?: string
}

export function EmptyState({ icon, title, description, code }: EmptyStateProps) {
  return (
    <div className="w-full py-10">
      <div className="mx-auto max-w-[520px]">
        <div className="rounded-xl border bg-card px-6 py-10 text-card-foreground shadow-sm">
          <div className="flex flex-col items-center text-center gap-3">
            <div className="size-12 rounded-xl bg-primary/10 text-primary flex items-center justify-center [&_svg]:size-6 [&_svg]:fill-none [&_svg]:stroke-current [&_svg]:[stroke-width:2] [&_svg]:[stroke-linecap:round] [&_svg]:[stroke-linejoin:round]">
              {icon}
            </div>
            <div className="text-lg font-semibold text-foreground">
              {title}
            </div>
            {description && (
              <div className="text-sm text-muted-foreground whitespace-pre-wrap">
                {description}
              </div>
            )}
            {code && (
              <code className="mt-1 rounded-md border bg-muted px-2 py-1 text-xs font-mono text-foreground">
                {code}
              </code>
            )}
          </div>
        </div>
      </div>
    </div>
  )
}
