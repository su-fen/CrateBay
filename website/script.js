(() => {
  "use strict"
  const year = String(new Date().getFullYear())
  document.querySelectorAll("[data-year]").forEach((node) => {
    node.textContent = year
  })
})()
