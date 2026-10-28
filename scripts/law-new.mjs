#!/usr/bin/env node
/**
 * 新建立法变更单（L2/L3 专用）。
 * 用法：node scripts/law-new.mjs <slug>
 */
import { mkdir, readFile, writeFile } from 'node:fs/promises'
import { existsSync } from 'node:fs'
import path from 'node:path'

const ROOT = process.cwd()
const LAW_DIR = path.join(ROOT, 'docs', 'law-changes')
const TEMPLATE = path.join(LAW_DIR, '_template.md')
const SLUG = /^[a-z0-9][a-z0-9-]{0,40}$/

function timestamp() {
  const now = new Date()
  const pad = (value) => String(value).padStart(2, '0')
  return `${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}_${pad(now.getHours())}-${pad(now.getMinutes())}`
}

async function main() {
  const slug = process.argv[2]
  if (!slug) throw new Error('用法：node scripts/law-new.mjs <slug>')
  if (!SLUG.test(slug)) throw new Error(`slug 只能是小写字母、数字和连字符：${slug}`)

  const id = `${timestamp()}_${slug}`
  const target = path.join(LAW_DIR, `${id}.md`)
  if (existsSync(target)) throw new Error(`变更单已存在：docs/law-changes/${id}.md`)

  const template = await readFile(TEMPLATE, 'utf8')
  const body = template.replace(/^id:\s*.*$/m, `id: ${id}`)
  await mkdir(LAW_DIR, { recursive: true })
  await writeFile(target, body)

  console.log(`已创建立法变更单：docs/law-changes/${id}.md`)
  console.log('填写现状与提议、影响面、判据与退役条件，等用户批准后再改 .cursor/rules/law/*.mdc。')
}

try {
  await main()
} catch (error) {
  console.error(`law-new: ${error.message}`)
  process.exitCode = 1
}
