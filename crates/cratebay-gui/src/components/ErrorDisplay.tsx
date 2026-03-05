import { I } from "../icons"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"

interface ErrorBannerProps {
  title: string
  message: string
  actionLabel?: string
  onAction?: () => void
}

export function ErrorBanner({ title, message, actionLabel, onAction }: ErrorBannerProps) {
  return (
    <Alert variant="destructive">
      <div className="flex items-start gap-3">
        <div className="mt-0.5 text-destructive [&_svg]:size-4 [&_svg]:fill-none [&_svg]:stroke-current [&_svg]:[stroke-width:2] [&_svg]:[stroke-linecap:round] [&_svg]:[stroke-linejoin:round]">
          {I.alertCircle}
        </div>
        <div className="min-w-0 flex-1">
          <AlertTitle>{title}</AlertTitle>
          <AlertDescription>
            <p className="whitespace-pre-wrap">{message}</p>
          </AlertDescription>
        </div>
        {actionLabel && onAction && (
          <Button
            type="button"
            variant="outline"
            size="sm"
            className="border-destructive/30 text-destructive hover:bg-destructive/10"
            onClick={onAction}
          >
            <span className="mr-1 [&_svg]:size-4 [&_svg]:fill-none [&_svg]:stroke-current [&_svg]:[stroke-width:2] [&_svg]:[stroke-linecap:round] [&_svg]:[stroke-linejoin:round]">
              {I.refresh}
            </span>
            {actionLabel}
          </Button>
        )}
      </div>
    </Alert>
  )
}

interface ErrorInlineProps {
  message: string
  onDismiss: () => void
}

export function ErrorInline({ message, onDismiss }: ErrorInlineProps) {
  return (
    <Alert variant="destructive" className="py-2 pr-10">
      <div className="absolute right-2 top-2">
        <Button
          type="button"
          variant="ghost"
          size="icon-xs"
          className="hover:bg-destructive/10 hover:text-destructive"
          onClick={onDismiss}
          aria-label="Close"
          data-testid="error-inline-dismiss"
        >
          ×
        </Button>
      </div>
      <div className="text-destructive [&_svg]:size-4 [&_svg]:fill-none [&_svg]:stroke-current [&_svg]:[stroke-width:2] [&_svg]:[stroke-linecap:round] [&_svg]:[stroke-linejoin:round]">
        {I.alertCircle}
      </div>
      <AlertTitle className="sr-only">Error</AlertTitle>
      <AlertDescription className="text-destructive/90">
        <p className="whitespace-pre-wrap">{message}</p>
      </AlertDescription>
    </Alert>
  )
}
