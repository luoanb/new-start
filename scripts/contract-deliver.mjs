#!/usr/bin/env node
/**
 * 交付：越界检查 + 证据校验 + 写锁，通过后置为 delivered。
 * 用法：node scripts/contract-deliver.mjs <id|目录>
 */
import { readFile, writeFile } from 'node:fs/promises'
import { spawnSync } from 'node:child_process'
import { fileURLToPath } from 'node:url'
import path from 'node:path'
import {
  changedFiles,
  matchScope,
  parseFrontmatter,
  resolveContractDir,
  setFrontmatterField,
  toPosix,
} from './contract-shared.mjs'

const HERE = path.dirname(fileURLToPath(import.meta.url))
const LINT = path.join(HERE, 'contract-lint.mjs')
const DRIFT = path.join(HERE, 'contract-drift.mjs')
const IGNORED_PREFIXES = ['docs/law-changes/']

async function main() {
  const argument = process.argv[2]
  if (!argument) throw new Error('用法：node scripts/contract-deliver.mjs <id|目录>')
  const dir = resolveContractDir(argument)
  const file = path.join(dir, 'contract.md')
  const original = await readFile(file, 'utf8')
  const { data } = parseFrontmatter(original, file)

  if (String(data.status) !== 'contracted') {
    throw new Error(`当前状态 ${data.status}：必须先由人审执行 contract:approve`)
  }
  if (String(data.approved_version) !== String(data.contract_version)) {
    throw new Error(
      `契约 v${data.contract_version} 尚未获批（approved_version=v${data.approved_version}）：先重新 approve`,
    )
  }
  if (!data.approved_commit || data.approved_commit === 'none') {
    throw new Error('缺少 approved_commit：契约未经过 approve')
  }

  const ownPrefix = `docs/contracts/${path.basename(dir)}/`
  const changed = changedFiles(String(data.approved_commit)).filter(
    (file) => !toPosix(file).startsWith(ownPrefix) && !IGNORED_PREFIXES.some((prefix) => toPosix(file).startsWith(prefix)),
  )
  const violations = changed.filter((file) => !matchScope(file, data.scope ?? []))
  if (violations.length > 0) {
    console.error('改动越出 scope，交付终止：')
    for (const file of violations) console.error(`  ${file}`)
    console.error('缩小改动，或升 contract_version 并重新 approve 后再交付。')
    process.exitCode = 1
    return
  }

  const delivered = setFrontmatterField(original, 'status', 'delivered')
  await writeFile(file, delivered)

  const lint = spawnSync(process.execPath, [LINT, '--dir', dir], { encoding: 'utf8' })
  if (lint.status !== 0) {
    await writeFile(file, original)
    console.error(`${lint.stdout ?? ''}${lint.stderr ?? ''}`.trim())
    console.error('证据或契约结构未通过，状态已回退为 contracted。')
    process.exitCode = 1
    return
  }

  const lock = spawnSync(process.execPath, [DRIFT, '--lock', dir], { encoding: 'utf8' })
  if (lock.status !== 0) {
    await writeFile(file, original)
    console.error(`${lock.stdout ?? ''}${lock.stderr ?? ''}`.trim())
    console.error('写锁失败，状态已回退为 contracted。')
    process.exitCode = 1
    return
  }

  console.log(`已交付 ${data.id} v${data.contract_version}`)
  console.log(`  scope 内改动 ${changed.length} 个文件，证据与 lint 通过`)
  console.log(`  ${(lock.stdout ?? '').trim()}`)
  console.log(`下一步由人验收：pnpm contract:accept ${data.id}`)
}

try {
  await main()
} catch (error) {
  console.error(`contract-deliver: ${error.message}`)
  process.exitCode = 1
}
