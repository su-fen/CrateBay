import fs from "node:fs"
import path from "node:path"
import { fileURLToPath } from "node:url"

const __dirname = path.dirname(fileURLToPath(import.meta.url))
const messagesPath = path.join(__dirname, "..", "src", "i18n", "messages.ts")

const src = fs.readFileSync(messagesPath, "utf8")
const lines = src.split(/\r?\n/)

function topLevelLangs() {
  let inMessages = false
  let depth = 0
  const langs = new Set()

  for (const line of lines) {
    if (!inMessages) {
      if (line.includes("export const messages")) {
        inMessages = true
        depth += (line.match(/\{/g) || []).length - (line.match(/\}/g) || []).length
      }
      continue
    }

    if (depth === 1) {
      const m = line.match(/^\s*([A-Za-z0-9_]+)\s*:\s*\{/)
      if (m) langs.add(m[1])
    }

    depth += (line.match(/\{/g) || []).length - (line.match(/\}/g) || []).length
    if (inMessages && depth <= 0) break
  }

  return langs
}

function extractLangKeys(lang) {
  let inBlock = false
  let depth = 0
  const keys = []

  const startRe = new RegExp(`^\\s*${lang}\\s*:\\s*\\{`)
  for (const line of lines) {
    if (!inBlock) {
      if (startRe.test(line)) {
        inBlock = true
        depth += (line.match(/\{/g) || []).length - (line.match(/\}/g) || []).length
      }
      continue
    }

    const stripped = line.trimStart()
    if (!(stripped.startsWith("//") || stripped.startsWith("/*") || stripped.startsWith("*"))) {
      const m = line.match(/^\s*([A-Za-z0-9_]+)\s*:\s*/)
      if (m) keys.push(m[1])
    }

    depth += (line.match(/\{/g) || []).length - (line.match(/\}/g) || []).length
    if (depth <= 0) break
  }

  return keys
}

const langs = topLevelLangs()
const expected = new Set(["en", "zh"])
const missingLangs = [...expected].filter((l) => !langs.has(l))
const extraLangs = [...langs].filter((l) => !expected.has(l))
if (missingLangs.length || extraLangs.length) {
  if (missingLangs.length) console.error(`ERROR: i18n: missing language blocks: ${missingLangs.join(", ")}`)
  if (extraLangs.length) console.error(`ERROR: i18n: unsupported language blocks: ${extraLangs.join(", ")}`)
  process.exit(1)
}

const enKeys = new Set(extractLangKeys("en"))
const zhKeys = new Set(extractLangKeys("zh"))
const missingInZh = [...enKeys].filter((k) => !zhKeys.has(k)).sort()
const missingInEn = [...zhKeys].filter((k) => !enKeys.has(k)).sort()

if (missingInZh.length || missingInEn.length) {
  if (missingInZh.length) console.error(`ERROR: i18n: missing zh keys: ${missingInZh.join(", ")}`)
  if (missingInEn.length) console.error(`ERROR: i18n: missing en keys: ${missingInEn.join(", ")}`)
  process.exit(1)
}

console.log("i18n OK: en/zh keys are in sync")

