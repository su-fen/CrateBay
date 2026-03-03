import { useState, useEffect, useCallback } from "react"

export function useToast() {
  const [toast, setToast] = useState("")

  useEffect(() => {
    if (!toast) return
    const tmr = setTimeout(() => setToast(""), 2200)
    return () => clearTimeout(tmr)
  }, [toast])

  const showToast = useCallback((msg: string) => setToast(msg), [])

  return { toast, showToast }
}
