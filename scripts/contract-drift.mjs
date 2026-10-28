#!/usr/bin/env node
/**
 * 契约漂移检测：scope 内代码已变、但契约版本未变时告警。
 *
 * 用法：
 *   node scripts/contract-drift.mjs               # 检查（CI 使用）
 *   node scripts/contract-drift.mjs --lock <dir>  # 为单个契约写锁
 *   node scripts/contract-drift.mjs --lock-all    # 为全部非归档契约写锁
 *
 * 退出码：0 无漂移；1 存在漂移或锁缺失。
 */
import { createHash } from 'node:crypto'
import { readdir, readFile, stat, writeFile } from 'node:fs/promises'
import path from 'node:path'
import {
  CONTRACTS_DIR,
  globToRegExp,
  listContractDirs,
  readContract,
} from './contract-shared.mjs'

const ROOT = process.cwd()
const LOCK_FILE = 'contract.lock.json'
const WATCHED = new Set(['delivered', 'accepted'])
const IGNORED_DIRS = new Set([
  'node_modules',
  '.git',
  '.svelte-kit',
  '.artifacts',
  '.trae-cn',
  'dist',
  'build',
  'target',
  'coverage',
])

function toKey(absolute) {
  return path.relative(ROOT, absolute).split(path.sep).join('/')
}

/** scope 中第一个通配符之前的静态目录前缀，用于缩小遍历范围。 */
function staticPrefix(glob) {
  const index = glob.search(/[*?[]/)
  if (index === -1) return glob
  const slash = glob.lastIndexOf('/', index)
  return slash === -1 ? '' : glob.slice(0, slash)
}

async function walk(dir, files) {
  let entries
  try {
    entries = await readdir(dir, { withFileTypes: true })
  } catch {
    return
  }
  for (const entry of entries) {
    if (entry.isSymbolicLink()) continue
    const full = path.join(dir, entry.name)
    if (entry.isDirectory()) {
      if (IGNORED_DIRS.has(entry.name)) continue
      await walk(full, files)
    } else if (entry.isFile()) {
      files.push(full)
    }
  }
}

async function collectScopeFiles(scope) {
  const matched = new Map()
  for (const glob of scope) {
    const normalized = String(glob).split(path.sep).join('/')
    const matcher = globToRegExp(normalized)
    const prefix = staticPrefix(normalized)
    const base = path.join(ROOT, prefix)

    let isFile = false
    try {
      isFile = (await stat(base)).isFile()
    } catch {
      isFile = false
    }

    if (isFile) {
      const key = toKey(base)
      if (matcher.test(key)) matched.set(key, base)
      continue
    }

    const files = []
    await walk(base, files)
    for (const file of files) {
      const key = toKey(file)
      if (matcher.test(key)) matched.set(key, file)
    }
  }
  return matched
}

async function hashFiles(files) {
  const result = {}
  for (const [key, absolute] of [...files.entries()].sort((a, b) => a[0].localeCompare(b[0]))) {
    const content = await readFile(absolute)
    result[key] = createHash('sha256').update(content).digest('hex')
  }
  return result
}

async function lockContract(name, dir) {
  const { data } = await readContract(dir)
  const files = await collectScopeFiles(data.scope ?? [])
  const lock = {
    contract: name,
    contract_version: Number(data.contract_version),
    locked_at: new Date().toISOString(),
    files: await hashFiles(files),
  }
  await writeFile(path.join(dir, LOCK_FILE), `${JSON.stringify(lock, null, 2)}\n`)
  return Object.keys(lock.files).length
}

async function checkContract(name, dir) {
  const problems = []
  const add = (file, message) => problems.push({ file, message })
  const { data } = await readContract(dir)
  const status = String(data.status)
  if (!WATCHED.has(status)) return problems

  const lockPath = path.join(dir, LOCK_FILE)
  let lock
  try {
    lock = JSON.parse(await readFile(lockPath, 'utf8'))
  } catch {
    add(toKey(lockPath), '已交付契约缺少有效的 contract.lock.json（运行 pnpm contract:lock）')
    return problems
  }

  const version = Number(data.contract_version)
  if (Number(lock.contract_version) !== version) {
    add(
      toKey(lockPath),
      `契约版本 ${version} 与锁内版本 ${lock.contract_version} 不一致：契约已变更，请重新 lock`,
    )
    return problems
  }

  const current = await hashFiles(await collectScopeFiles(data.scope ?? []))
  const locked = lock.files ?? {}

  for (const [file, hash] of Object.entries(current)) {
    if (!(file in locked)) add(file, 'scope 内新增文件，契约未变更')
    else if (locked[file] !== hash) add(file, 'scope 内文件已修改，契约未变更')
  }
  for (const file of Object.keys(locked)) {
    if (!(file in current)) add(file, 'scope 内文件已删除，契约未变更')
  }
  return problems
}

async function main() {
  const lockAll = process.argv.includes('--lock-all')
  const lockFlag = process.argv.indexOf('--lock')
  const lockDir = lockFlag === -1 ? null : process.argv[lockFlag + 1]

  if (lockFlag !== -1 && !lockDir) {
    console.error('contract-drift: --lock 需要一个契约目录')
    process.exit(1)
  }

  if (lockDir) {
    const absolute = path.resolve(lockDir)
    const count = await lockContract(path.basename(absolute), absolute)
    console.log(`contract-drift: 已锁定 ${toKey(absolute)}（${count} 个文件）`)
    return
  }

  if (lockAll) {
    let total = 0
    for (const name of await listContractDirs()) {
      total += await lockContract(name, path.join(CONTRACTS_DIR, name))
    }
    console.log(`contract-drift: 已锁定全部契约（${total} 个文件）`)
    return
  }

  const problems = []
  const names = await listContractDirs()
  for (const name of names) {
    try {
      problems.push(...(await checkContract(name, path.join(CONTRACTS_DIR, name))))
    } catch (error) {
      problems.push({ file: `docs/contracts/${name}`, message: error.message })
    }
  }

  if (problems.length === 0) {
    console.log(`contract-drift: ${names.length} 个契约无漂移`)
    return
  }
  for (const problem of problems) console.error(`${problem.file}: ${problem.message}`)
  console.error(`contract-drift: ${problems.length} 处漂移`)
  process.exitCode = 1
}

await main()
