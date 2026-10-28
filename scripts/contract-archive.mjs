#!/usr/bin/env node
/**
 * 归档：把已验收契约移入 docs/contracts/archive/。
 * 用法：node scripts/contract-archive.mjs <id|目录>
 */
import { mkdir, readFile, rename } from 'node:fs/promises'
import { existsSync } from 'node:fs'
import path from 'node:path'
import { CONTRACTS_DIR, parseFrontmatter, resolveContractDir } from './contract-shared.mjs'

async function main() {
  const argument = process.argv[2]
  if (!argument) throw new Error('用法：node scripts/contract-archive.mjs <id|目录>')
  const dir = resolveContractDir(argument)
  const file = path.join(dir, 'contract.md')
  const { data } = parseFrontmatter(await readFile(file, 'utf8'), file)

  if (String(data.status) !== 'accepted') {
    throw new Error(`当前状态 ${data.status}：只有 accepted 可以归档`)
  }

  const archiveDir = path.join(CONTRACTS_DIR, 'archive')
  const target = path.join(archiveDir, path.basename(dir))
  if (existsSync(target)) throw new Error(`归档目标已存在：${path.relative(process.cwd(), target)}`)
  await mkdir(archiveDir, { recursive: true })
  await rename(dir, target)

  console.log(`已归档：${path.relative(process.cwd(), target)}`)
  console.log('归档契约不被后续需求引用，也不沉淀为法；记得提交此次移动。')
}

try {
  await main()
} catch (error) {
  console.error(`contract-archive: ${error.message}`)
  process.exitCode = 1
}
