#!/usr/bin/env node
/**
 * 新建语义契约骨架（AI 立项入口）。
 * 用法：node scripts/contract-new.mjs <slug> --scope <glob> [--scope <glob>] [--auth L0|L1]
 */
import { mkdir, readFile, writeFile } from 'node:fs/promises'
import { existsSync } from 'node:fs'
import path from 'node:path'
import { CONTRACTS_DIR, parseFrontmatter } from './contract-shared.mjs'

const SLUG = /^[a-z0-9][a-z0-9-]{0,40}$/
const TEMPLATE_DIR = path.join(CONTRACTS_DIR, '_template')

function parseArgs(argv) {
  const options = { slug: null, auth: 'L1', scopes: [] }
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index]
    if (arg === '--auth') {
      options.auth = argv[index + 1]
      index += 1
      continue
    }
    if (arg === '--scope') {
      options.scopes.push(argv[index + 1])
      index += 1
      continue
    }
    if (arg.startsWith('--')) throw new Error(`未知参数：${arg}`)
    if (options.slug) throw new Error('只接受一个 slug')
    options.slug = arg
  }
  return options
}

function timestamp() {
  const now = new Date()
  const pad = (value) => String(value).padStart(2, '0')
  const date = `${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}`
  const time = `${pad(now.getHours())}-${pad(now.getMinutes())}`
  return {
    id: `${date}_${time}`,
    createdAt: `${date} ${time.replace('-', ':')}`,
  }
}

async function readTemplateBody(file) {
  const text = await readFile(file, 'utf8')
  return parseFrontmatter(text, file).body
}

async function main() {
  const { slug, auth, scopes } = parseArgs(process.argv.slice(2))
  if (!slug) throw new Error('缺少 slug：node scripts/contract-new.mjs <slug> --scope <glob>')
  if (!SLUG.test(slug)) throw new Error(`slug 只能是小写字母、数字和连字符：${slug}`)
  if (!['L0', 'L1'].includes(auth)) {
    throw new Error(`L2/L3 不属于需求迭代，请改用 pnpm law:new <slug>（当前 --auth ${auth}）`)
  }
  const validScopes = scopes.filter(Boolean)
  if (validScopes.length === 0) throw new Error('至少给一个 --scope（允许改动的路径 glob）')

  const { id, createdAt } = timestamp()
  const contractId = `${id}_${slug}`
  const dir = path.join(CONTRACTS_DIR, contractId)
  if (existsSync(dir)) throw new Error(`契约已存在：${contractId}`)
  await mkdir(dir, { recursive: true })

  const frontmatter = [
    '---',
    `id: ${contractId}`,
    'status: draft',
    `authorization: ${auth}`,
    'contract_version: 1',
    'approved_version: 0',
    'approved_commit: none',
    'scope:',
    ...validScopes.map((scope) => `  - ${scope}`),
    'depends_on: []',
    `created_at: ${createdAt}`,
    '---',
    '',
  ].join('\n')

  const contractBody = (await readTemplateBody(path.join(TEMPLATE_DIR, 'contract.md'))).replace(
    /^# 语义契约：.*$/m,
    `# 语义契约：${slug}`,
  )
  await writeFile(path.join(dir, 'contract.md'), `${frontmatter}\n${contractBody.trimStart()}`)

  const evidenceBody = await readTemplateBody(path.join(TEMPLATE_DIR, 'evidence.md'))
  const evidenceFrontmatter = [
    '---',
    `contract: ${contractId}`,
    'contract_version: 1',
    'result: pending',
    '---',
    '',
  ].join('\n')
  await writeFile(path.join(dir, 'evidence.md'), `${evidenceFrontmatter}\n${evidenceBody.trimStart()}`)

  console.log(`已创建契约：docs/contracts/${contractId}/`)
  console.log('下一步：')
  console.log('  1. 填 contract.md 的 E/S/I/B/A 条目（每条必须有判定方式）')
  console.log(`  2. pnpm contract:lint（或 node scripts/contract-lint.mjs --dir docs/contracts/${contractId}）`)
  console.log(`  3. 交人审：pnpm contract:review ${contractId}`)
}

try {
  await main()
} catch (error) {
  console.error(`contract-new: ${error.message}`)
  process.exitCode = 1
}
