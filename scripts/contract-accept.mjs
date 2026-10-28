#!/usr/bin/env node
/**
 * 验收：人确认交付结果，置为 accepted。
 * 用法：node scripts/contract-accept.mjs <id|目录>
 */
import { readFile, writeFile } from 'node:fs/promises'
import { spawnSync } from 'node:child_process'
import { fileURLToPath } from 'node:url'
import path from 'node:path'
import { parseFrontmatter, resolveContractDir, setFrontmatterField } from './contract-shared.mjs'

const LINT = path.join(path.dirname(fileURLToPath(import.meta.url)), 'contract-lint.mjs')

async function main() {
  const argument = process.argv[2]
  if (!argument) throw new Error('用法：node scripts/contract-accept.mjs <id|目录>')
  const dir = resolveContractDir(argument)
  const file = path.join(dir, 'contract.md')
  const text = await readFile(file, 'utf8')
  const { data } = parseFrontmatter(text, file)

  if (String(data.status) !== 'delivered') {
    throw new Error(`当前状态 ${data.status}：只有 delivered 可以验收`)
  }
  const lint = spawnSync(process.execPath, [LINT, '--dir', dir], { encoding: 'utf8' })
  if (lint.status !== 0) {
    console.error(`${lint.stdout ?? ''}${lint.stderr ?? ''}`.trim())
    throw new Error('当前交付未通过 lint，不能验收')
  }

  await writeFile(file, setFrontmatterField(text, 'status', 'accepted'))
  console.log(`已验收 ${data.id} v${data.contract_version}`)
  console.log(`下一步：pnpm contract:archive ${data.id}`)
}

try {
  await main()
} catch (error) {
  console.error(`contract-accept: ${error.message}`)
  process.exitCode = 1
}
