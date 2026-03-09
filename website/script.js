(() => {
  "use strict"

  const translations = {
    en: {
      lang: "en",
      title: "CrateBay — Local AI control plane in one GUI · Coming Soon",
      description:
        "CrateBay — a desktop GUI for local AI sandboxes, local models, and MCP servers.",
      keywords: "cratebay, local ai, ai sandbox, local models, mcp, desktop gui",
      brand: "CrateBay",
      comingSoon: "Coming Soon",
      heroTitle: "Your local AI stack, in one GUI.",
      heroLead:
        "Managed sandboxes. Local models. MCP — all in one desktop app.",
      heroSub:
        "Built for fast local AI workflows with safer defaults and clearer runtime feedback.",
      githubCta: "GitHub",
      summary1Label: "AI Sandboxes",
      summary1Title: "Run locally",
      summary1Body:
        "Create, start, stop, inspect, and manage built-in local AI sandboxes with safer confirmations and clearer runtime feedback.",
      summary2Label: "Local Models",
      summary2Title: "One-click local models",
      summary2Body:
        "Pull, manage, and run local models from the same desktop surface.",
      summary3Label: "MCP + Tools",
      summary3Title: "Connect tools fast",
      summary3Body:
        "Run MCP servers and expose sandboxes as MCP tools for your AI clients.",
      sectionKicker: "Why It Hits",
      sectionTitle: "Local AI is hot. The workflow is still broken.",
      sectionBody:
        "Bring up a model, start a managed sandbox, connect MCP tools, and iterate — all inside one desktop GUI.",
      card1Title: "Sandbox-first GUI",
      card1Body:
        "Managed sandboxes are a first-class desktop workflow with clearer guardrails for exec, cleanup, and delete actions.",
      card2Title: "One-click local models",
      card2Body:
        "Make local model setup fast, visual, and daily-use ready.",
      card3Title: "MCP built in",
      card3Body:
        "Manage MCP servers, and connect AI clients to sandboxes via MCP tools.",
      card4Title: "Focus on the AI workflow",
      card4Body:
        "Keep your local AI workflow focused, predictable, and easy to manage.",
      statusKicker: "Status",
      statusTitle: "Coming soon",
      statusBody:
        "For builders who want a better local AI workflow.",
      statusNote:
        "Preview builds are evolving quickly. Follow the GitHub repo for updates and release notes.",
      footer: "CrateBay · <span data-year></span>",
    },
    zh: {
      lang: "zh-CN",
      title: "CrateBay — 本地 AI 控制台，一体化桌面 GUI · 即将推出",
      description:
        "CrateBay —— 面向本地 AI 沙箱、本地模型与 MCP Server 的一体化桌面 GUI。",
      keywords: "cratebay, local ai, ai sandbox, 本地模型, mcp, desktop gui",
      brand: "CrateBay",
      comingSoon: "即将推出",
      heroTitle: "你的本地 AI 栈，一个 GUI 搞定。",
      heroLead:
        "托管沙箱、本地模型、MCP —— 全都放进一个桌面应用里。",
      heroSub:
        "为更快的本地 AI 工作流而生：默认更安全，反馈更清晰。",
      githubCta: "GitHub",
      summary1Label: "AI Sandboxes",
      summary1Title: "本地运行，可视化管理",
      summary1Body:
        "用桌面 GUI 创建、启动、停止、检查并管理由 CrateBay 托管的本地 AI 沙箱，同时获得更安全的确认提示与更清晰的运行时反馈。",
      summary2Label: "Local Models",
      summary2Title: "一键本地模型",
      summary2Body:
        "在同一个桌面界面里拉取、管理和运行本地模型。",
      summary3Label: "MCP + 工具",
      summary3Title: "快速连通工具链",
      summary3Body:
        "把 MCP Server 与沙箱能力统一到一个桌面控制面，并对外提供 MCP tools。",
      sectionKicker: "为什么它有吸引力",
      sectionTitle: "本地 AI 很火，但真正顺手的工作流还不多。",
      sectionBody:
        "拉起模型、启动托管沙箱、连接 MCP 工具并快速迭代，都在一个桌面 GUI 里完成。",
      card1Title: "Sandbox-first GUI",
      card1Body:
        "由 CrateBay 托管的 AI 沙箱，不该只是 CLI 背后的专家功能；exec、清理与删除等动作也需要更清晰的保护栏。",
      card2Title: "一键本地模型",
      card2Body:
        "让本地模型部署与管理更快、更直观。",
      card3Title: "桌面内建 MCP",
      card3Body:
        "管理 MCP Server，并通过 MCP tools 把沙箱能力对外开放给 AI 客户端。",
      card4Title: "聚焦本地 AI 工作流",
      card4Body:
        "让你的本地 AI 工作流更聚焦、更可控、更易管理。",
      statusKicker: "状态",
      statusTitle: "即将推出",
      statusBody:
        "如果你想要更好的本地 AI 工作流，就关注 CrateBay。",
      statusNote:
        "预览版本迭代很快；更新与发布说明请关注 GitHub 仓库。",
      footer: "CrateBay · <span data-year></span>",
    },
  }

  const storageKey = "cratebay-site-lang"
  const titleNode = document.querySelector("title")
  const descriptionMeta = document.querySelector('meta[name="description"]')
  const keywordsMeta = document.querySelector('meta[name="keywords"]')
  const year = String(new Date().getFullYear())

  function renderFooter() {
    document.querySelectorAll("[data-year]").forEach((node) => {
      node.textContent = year
    })
  }

  function setLanguage(lang) {
    const next = translations[lang] ? lang : "en"
    const dict = translations[next]
    document.documentElement.lang = dict.lang
    if (titleNode) titleNode.textContent = dict.title
    if (descriptionMeta) descriptionMeta.setAttribute("content", dict.description)
    if (keywordsMeta) keywordsMeta.setAttribute("content", dict.keywords)

    document.querySelectorAll("[data-i18n]").forEach((node) => {
      const key = node.getAttribute("data-i18n")
      if (!key || !(key in dict)) return
      if (key === "footer") {
        node.innerHTML = dict[key]
      } else {
        node.textContent = dict[key]
      }
    })

    document.querySelectorAll(".lang-btn").forEach((button) => {
      const active = button.getAttribute("data-lang") === next
      button.setAttribute("aria-pressed", active ? "true" : "false")
    })

    renderFooter()
    localStorage.setItem(storageKey, next)
  }

  const saved = localStorage.getItem(storageKey)
  const initial = saved || (navigator.language && navigator.language.toLowerCase().startsWith("zh") ? "zh" : "en")
  setLanguage(initial)

  document.querySelectorAll(".lang-btn").forEach((button) => {
    button.addEventListener("click", () => {
      const lang = button.getAttribute("data-lang") || "en"
      setLanguage(lang)
    })
  })
})()
