#!/usr/bin/env node
/**
 * 校验 docs/contracts 下的语义契约与守约证据。
 *
 * 用法：
 *   node scripts/contract-lint.mjs                # 校验全部契约
 *   node scripts/contract-lint.mjs --dir <path>   # 只校验一个契约目录
 *
 * 退出码：0 通过；1 存在结构错误。
 */
import path from 'node:path'
import { readFile } from 'node:fs/promises'
import {
  CONTRACTS_DIR,
  CATEGORY_ID,
  listContractDirs,
  parseFrontmatter,
  readContract,
  tableRows,
} from './contract-shared.mjs'

const STATUSES = new Set(['draft', 'contracted', 'delivered', 'accepted'])
const AUTHORIZATIONS = new Set(['L0', 'L1', 'L2', 'L3'])
const REQUIRED_CATEGORIES = ['E', 'I', 'B', 'A']
const PLACEHOLDER = /(TODO|TBD|待定|待填写|待补)/i
const DELIVERED = new Set(['delivered', 'accepted'])

function isFilled(value) {
  return typeof value === 'string' && value.trim() !== '' && !PLACEHOLDER.test(value)
}

function relative(file) {
  return path.relative(process.cwd(), file)
}

async function validateContract(dirName, dir) {
  const errors = []
  const add = (file, message) => errors.push({ file, message })
  const { file, data, body } = await readContract(dir)
  const rel = relative(file)
  const status = String(data.status)

  if (data.id !== dirName) {
    add(rel, `frontmatter id「${data.id ?? ''}」必须等于目录名「${dirName}」`)
  }
  if (status === 'archived') {
    add(rel, 'status 不使用 archived：归档请把整个目录移入 docs/contracts/archive/')
  } else if (!STATUSES.has(status)) {
    add(rel, `status 必须是 ${[...STATUSES].join(' / ')}，当前为「${data.status ?? ''}」`)
  }
  if (!AUTHORIZATIONS.has(String(data.authorization))) {
    add(rel, `authorization 必须是 ${[...AUTHORIZATIONS].join(' / ')}，当前为「${data.authorization ?? ''}」`)
  }
  if (['L2', 'L3'].includes(String(data.authorization))) {
    add(rel, '契约只接受 L0/L1：L2/L3 触碰契约或边界，必须走 docs/law-changes/')
  }

  const version = Number(data.contract_version)
  if (!Number.isInteger(version) || version < 1) {
    add(rel, `contract_version 必须是 ≥1 的整数，当前为「${data.contract_version ?? ''}」`)
  }
  const approvedVersion = Number(data.approved_version)
  if (!Number.isInteger(approvedVersion) || approvedVersion < 0) {
    add(rel, `approved_version 必须是 ≥0 的整数，当前为「${data.approved_version ?? ''}」`)
  }
  if (typeof data.approved_commit !== 'string' || data.approved_commit === '') {
    add(rel, 'approved_commit 必须存在（未批准写 none）')
  }
  if (['contracted', 'delivered', 'accepted'].includes(status)) {
    if (approvedVersion !== version) {
      add(rel, `契约 v${version} 尚未获批（approved_version=v${approvedVersion}）：先 contract:approve`)
    }
    if (data.approved_commit === 'none') {
      add(rel, '已批准契约必须记录 approved_commit')
    }
  }
  if (!Array.isArray(data.scope) || data.scope.length === 0) {
    add(rel, 'scope 不能为空：至少声明一个允许改动的路径 glob')
  } else if (data.scope.some((item) => !isFilled(item))) {
    add(rel, 'scope 中存在空项或占位符')
  }
  if (!Array.isArray(data.depends_on)) {
    add(rel, 'depends_on 必须显式写为 []')
  } else if (data.depends_on.length > 0) {
    add(rel, 'depends_on 必须为空：契约自包含，不引用其他需求')
  }

  const rows = []
  for (const row of tableRows(body)) {
    if (!CATEGORY_ID.test(row.cells[0])) continue
    const [id, statement, verify] = row.cells
    if (!isFilled(statement)) add(rel, `第 ${row.lineNumber} 行 ${id}：陈述为空或含占位符`)
    if (!isFilled(verify)) add(rel, `第 ${row.lineNumber} 行 ${id}：判定方式为空或含占位符`)
    rows.push({ id, lineNumber: row.lineNumber })
  }

  const ids = new Set()
  for (const row of rows) {
    if (ids.has(row.id)) add(rel, `条目 ID 重复：${row.id}`)
    ids.add(row.id)
  }
  for (const category of REQUIRED_CATEGORIES) {
    if (!rows.some((row) => row.id.startsWith(category))) {
      add(rel, `缺少 ${category} 类条目（E 实体 / I 不变量 / B 边界 / A 验收 必须各至少一条）`)
    }
  }

  if (DELIVERED.has(String(data.status))) {
    await validateEvidence(dirName, dir, version, ids, add)
  }

  return errors
}

async function validateEvidence(dirName, dir, version, contractIds, add) {
  const file = path.join(dir, 'evidence.md')
  let text
  try {
    text = await readFile(file, 'utf8')
  } catch {
    add(relative(file), 'delivered / accepted 状态必须提供 evidence.md')
    return
  }

  const rel = relative(file)
  const { data, body } = parseFrontmatter(text, rel)
  if (data.contract !== dirName) add(rel, `frontmatter contract 必须是「${dirName}」`)
  if (String(data.contract_version) !== String(version)) {
    add(rel, `contract_version 必须与契约一致（${version}）`)
  }
  if (String(data.result) !== 'pass') add(rel, '整体 result 必须是 pass')

  const evidenceIds = new Set()
  for (const row of tableRows(body)) {
    if (!CATEGORY_ID.test(row.cells[0])) continue
    const [id, kind, evidence, result] = row.cells
    if (![kind, evidence, result].every(isFilled)) {
      add(rel, `第 ${row.lineNumber} 行 ${id}：证据类型 / 命令或文件 / 结果 不得为空`)
    }
    if (String(result).trim().toLowerCase() !== 'pass') {
      add(rel, `第 ${row.lineNumber} 行 ${id}：结果必须是 pass，当前为「${result ?? ''}」`)
    }
    evidenceIds.add(id)
  }

  for (const id of contractIds) {
    if (!evidenceIds.has(id)) add(rel, `缺少契约条目 ${id} 的证据`)
  }
  for (const id of evidenceIds) {
    if (!contractIds.has(id)) add(rel, `证据引用了契约中不存在的条目 ${id}`)
  }
}

async function main() {
  const dirFlag = process.argv.indexOf('--dir')
  const onlyDir = dirFlag === -1 ? null : process.argv[dirFlag + 1]
  if (dirFlag !== -1 && !onlyDir) {
    console.error('contract-lint: --dir 需要一个路径参数')
    process.exit(1)
  }

  const targets = onlyDir
    ? [path.basename(path.resolve(onlyDir))]
    : await listContractDirs()

  const allErrors = []
  for (const name of targets) {
    const dir = onlyDir ? path.resolve(onlyDir) : path.join(CONTRACTS_DIR, name)
    try {
      allErrors.push(...(await validateContract(name, dir)))
    } catch (error) {
      allErrors.push({ file: relative(path.join(dir, 'contract.md')), message: error.message })
    }
  }

  if (allErrors.length === 0) {
    console.log(`contract-lint: ${targets.length} 个契约通过`)
    return
  }
  for (const error of allErrors) console.error(`${error.file}: ${error.message}`)
  console.error(`contract-lint: ${targets.length} 个契约，${allErrors.length} 个错误`)
  process.exitCode = 1
}

await main()
