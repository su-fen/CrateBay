import "@testing-library/jest-dom/vitest"
import "./mocks/tauri"

// jsdom doesn't implement scrollIntoView
Element.prototype.scrollIntoView = () => {}
