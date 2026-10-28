#!/usr/bin/env node
/**
 * 人审批准：冻结契约版本并记录批准时的 HEAD。
 * 用法：node scripts/contract-approve.mjs <id|目录>
 */
import { readFile, writeFile } from 'node:fs/promises'
import { spawnSync } from 'node:child_process'
import { fileURLToPath } from 'node:url'
import path from 'node:path'
import {
  headCommit,
  parseFrontmatter,
  resolveContractDir,
  setFrontmatterField,
} from './contract-shared.mjs'

const LINT = path.join(path.dirname(fileURLToPath(import.meta.url)), 'contract-lint.mjs')

async function main() {
  const argument = process.argv[2]
  if (!argument) throw new Error('用法：node scripts/contract-approve.mjs <id|目录>')
  const dir = resolveContractDir(argument)
  const file = path.join(dir, 'contract.md')
  const text = await readFile(file, 'utf8')
  const { data } = parseFrontmatter(text, file)

  if (!['draft', 'contracted'].includes(String(data.status))) {
    throw new Error(`当前状态 ${data.status} 不能 approve`)
  }
  if (!['L0', 'L1'].includes(String(data.authorization))) {
    throw new Error(`L2/L3 不属于需求迭代，请走 pnpm law:new`)
  }

  const lint = spawnSync(process.execPath, [LINT, '--dir', dir], { encoding: 'utf8' })
  if (lint.status !== 0) {
    console.error(`${lint.stdout ?? ''}${lint.stderr ?? ''}`.trim())
    throw new Error('lint 未通过，不能批准契约')
  }

  const version = Number(data.contract_version)
  const commit = headCommit()
  if (commit === 'none') throw new Error('需要 git 仓库：批准时要记录 HEAD 以做交付越界检查')

  let next = setFrontmatterField(text, 'approved_version', String(version))
  next = setFrontmatterField(next, 'approved_commit', commit)
  next = setFrontmatterField(next, 'status', 'contracted')
  await writeFile(file, next)

  console.log(`已批准 ${data.id} v${version}（commit ${commit.slice(0, 8)}）`)
  console.log('契约已冻结：修改必须升 contract_version 并重新 approve。')
  console.log(`下一步交给 AI：pnpm contract:deliver ${data.id}`)
}

try {
  await main()
} catch (error) {
  console.error(`contract-approve: ${error.message}`)
  process.exitCode = 1
}
