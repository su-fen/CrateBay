import { useState, useRef, useEffect } from "react"

interface Option {
  value: string
  label: string
}

interface CustomSelectProps {
  value: string
  options: Option[]
  onChange: (value: string) => void
}

export function CustomSelect({ value, options, onChange }: CustomSelectProps) {
  const [open, setOpen] = useState(false)
  const ref = useRef<HTMLDivElement>(null)

  const selected = options.find(o => o.value === value)

  useEffect(() => {
    const handler = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) {
        setOpen(false)
      }
    }
    document.addEventListener("mousedown", handler)
    return () => document.removeEventListener("mousedown", handler)
  }, [])

  return (
    <div className="custom-select" ref={ref}>
      <button
        type="button"
        className="custom-select-trigger"
        onClick={() => setOpen(!open)}
      >
        <span>{selected?.label ?? value}</span>
        <svg className="custom-select-arrow" viewBox="0 0 24 24">
          <polyline points="6 9 12 15 18 9" />
        </svg>
      </button>
      {open && (
        <div className="custom-select-dropdown">
          {options.map(o => (
            <div
              key={o.value}
              className={`custom-select-option${o.value === value ? " active" : ""}`}
              onClick={() => { onChange(o.value); setOpen(false) }}
            >
              {o.value === value && (
                <svg className="custom-select-check" viewBox="0 0 24 24">
                  <polyline points="20 6 9 17 4 12" />
                </svg>
              )}
              <span>{o.label}</span>
            </div>
          ))}
        </div>
      )}
    </div>
  )
}
