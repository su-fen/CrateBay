import "@testing-library/jest-dom/vitest"
import "./mocks/tauri"

// jsdom doesn't implement scrollIntoView
Element.prototype.scrollIntoView = () => {}

const originalConsoleError = console.error.bind(console)
const suppressedConsoleErrorPatterns = [/not wrapped in act/i]

console.error = (...args: unknown[]) => {
  const message = args
    .map((arg) => (typeof arg === "string" ? arg : String(arg)))
    .join(" ")
  if (suppressedConsoleErrorPatterns.some((pattern) => pattern.test(message))) {
    return
  }
  originalConsoleError(...args)
}
