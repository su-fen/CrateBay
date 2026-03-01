import { I } from "../icons"

interface ErrorBannerProps {
  title: string
  message: string
  actionLabel?: string
  onAction?: () => void
}

export function ErrorBanner({ title, message, actionLabel, onAction }: ErrorBannerProps) {
  return (
    <div className="error-msg">
      <div className="error-msg-icon">{I.alertCircle}</div>
      <div className="error-msg-title">{title}</div>
      <div className="error-msg-text">{message}</div>
      {actionLabel && onAction && (
        <div className="error-msg-action">
          <button className="btn" onClick={onAction}>
            <span className="icon">{I.refresh}</span>{actionLabel}
          </button>
        </div>
      )}
    </div>
  )
}

interface ErrorInlineProps {
  message: string
  onDismiss: () => void
}

export function ErrorInline({ message, onDismiss }: ErrorInlineProps) {
  return (
    <div className="error-inline">
      <div className="error-inline-icon">{I.alertCircle}</div>
      <div className="error-inline-text">{message}</div>
      <button className="error-inline-dismiss" onClick={onDismiss}>&times;</button>
    </div>
  )
}
