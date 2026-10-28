/**
 * 契约工具共享函数：frontmatter 解析、表格解析、契约目录枚举。
 * 只依赖 Node 标准库，供 contract-lint / contract-drift 复用。
 */
import { readdir, readFile } from 'node:fs/promises'
import { existsSync } from 'node:fs'
import { spawnSync } from 'node:child_process'
import path from 'node:path'

export const CONTRACTS_DIR = path.join(process.cwd(), 'docs', 'contracts')
export const CATEGORY_ID = /^(E|S|I|B|A)\d+$/
export const SKIP_DIRS = new Set(['_template', 'archive'])

export function unquote(value) {
  const trimmed = value.trim()
  const quoted =
    (trimmed.startsWith('"') && trimmed.endsWith('"')) ||
    (trimmed.startsWith("'") && trimmed.endsWith("'"))
  return quoted ? trimmed.slice(1, -1) : trimmed
}

export function parseFrontmatter(text, file) {
  if (!text.startsWith('---\n')) throw new Error(`${file}: 缺少 YAML frontmatter`)
  let end = text.indexOf('\n---\n', 4)
  let bodyStart
  if (end === -1) {
    if (!text.endsWith('\n---')) throw new Error(`${file}: frontmatter 未闭合`)
    end = text.length - 4
    bodyStart = text.length
  } else {
    bodyStart = end + 5
  }

  const data = {}
  let currentKey = null
  for (const line of text.slice(4, end).split('\n')) {
    if (!line.trim() || line.trim().startsWith('#')) continue
    const item = /^\s+-\s+(.*)$/.exec(line)
    if (item) {
      if (!currentKey) throw new Error(`${file}: 列表项缺少所属键：${line.trim()}`)
      if (!Array.isArray(data[currentKey])) data[currentKey] = []
      data[currentKey].push(unquote(item[1]))
      continue
    }
    const pair = /^([A-Za-z_][\w-]*):\s*(.*)$/.exec(line)
    if (!pair) throw new Error(`${file}: 无法解析 frontmatter 行：${line.trim()}`)
    currentKey = pair[1]
    const value = pair[2].trim()
    data[currentKey] = value === '' || value === '[]' ? [] : unquote(value)
  }
  return { data, body: text.slice(bodyStart) }
}

/** 解析 markdown 表格，返回每个数据行的单元格数组（跳过表头分隔行）。 */
export function tableRows(body) {
  const rows = []
  let lineNumber = 0
  for (const line of body.split('\n')) {
    lineNumber += 1
    const trimmed = line.trim()
    if (!trimmed.startsWith('|')) continue
    const cells = trimmed
      .replace(/^\|/, '')
      .replace(/\|$/, '')
      .split('|')
      .map((cell) => cell.trim())
    if (cells.length === 0) continue
    if (cells.every((cell) => /^:?-{3,}:?$/.test(cell))) continue
    rows.push({ cells, lineNumber })
  }
  return rows
}

/** 列出需校验的契约目录（排除模板与归档）。 */
export async function listContractDirs(baseDir = CONTRACTS_DIR) {
  const entries = await readdir(baseDir, { withFileTypes: true })
  return entries
    .filter((entry) => entry.isDirectory())
    .map((entry) => entry.name)
    .filter((name) => !SKIP_DIRS.has(name) && !name.startsWith('.'))
    .sort()
}

export async function readContract(dir) {
  const file = path.join(dir, 'contract.md')
  const text = await readFile(file, 'utf8')
  const { data, body } = parseFrontmatter(text, file)
  return { file, data, body }
}

/** 接受目录路径或契约 id，返回契约目录绝对路径。 */
export function resolveContractDir(argument) {
  const direct = path.resolve(argument)
  if (existsSync(path.join(direct, 'contract.md'))) return direct
  const nested = path.join(CONTRACTS_DIR, argument)
  if (existsSync(path.join(nested, 'contract.md'))) return nested
  throw new Error(`找不到契约：${argument}（可传目录路径或 docs/contracts 下的 id）`)
}

/** 只改 frontmatter 中的单行字段，正文不动。 */
export function setFrontmatterField(text, key, value) {
  const end = text.indexOf('\n---\n', 4)
  if (!text.startsWith('---\n') || end === -1) {
    throw new Error('frontmatter 结构异常，拒绝改写')
  }
  const head = text.slice(0, end)
  const rest = text.slice(end)
  const pattern = new RegExp(`^(${key}:\\s*).*$`, 'm')
  if (!pattern.test(head)) throw new Error(`frontmatter 缺少字段：${key}`)
  return head.replace(pattern, `$1${value}`) + rest
}

export function globToRegExp(glob) {
  let source = ''
  for (let index = 0; index < glob.length; index += 1) {
    const char = glob[index]
    if (char === '*') {
      if (glob[index + 1] === '*') {
        index += 1
        if (glob[index + 1] === '/') {
          index += 1
          source += '(?:.*/)?'
        } else {
          source += '.*'
        }
      } else {
        source += '[^/]*'
      }
    } else if (char === '?') {
      source += '[^/]'
    } else {
      source += char.replace(/[.+^${}()|[\]\\]/g, '\\$&')
    }
  }
  return new RegExp(`^${source}$`)
}

export function toPosix(file) {
  return file.split(path.sep).join('/')
}

/** 判断改动路径是否落在 scope 内。 */
export function matchScope(file, scope) {
  const key = toPosix(file)
  return scope.some((glob) => globToRegExp(toPosix(String(glob))).test(key))
}

function gitOutput(args) {
  const result = spawnSync('git', args, { encoding: 'utf8' })
  return result.status === 0 ? result.stdout : null
}

export function headCommit() {
  return (gitOutput(['rev-parse', 'HEAD']) ?? '').trim() || 'none'
}

/** 自某 commit 起已提交的改动 + 工作区改动（含未跟踪文件）。 */
export function changedFiles(sinceCommit) {
  const files = new Set()
  if (sinceCommit && sinceCommit !== 'none') {
    const committed = gitOutput(['diff', '--name-only', `${sinceCommit}..HEAD`]) ?? ''
    for (const line of committed.split('\n')) if (line.trim()) files.add(line.trim())
  }

  const status = gitOutput(['status', '--porcelain', '-z']) ?? ''
  const entries = status.split('\0').filter(Boolean)
  for (let index = 0; index < entries.length; index += 1) {
    const entry = entries[index]
    const code = entry.slice(0, 2)
    const file = entry.slice(3)
    if (!file) continue
    files.add(file)
    if (code.includes('R') || code.includes('C')) {
      index += 1
      const original = entries[index]
      if (original) files.add(original)
    }
  }
  return [...files]
}
