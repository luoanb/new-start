#!/usr/bin/env node
/**
 * 人审摘要：只展示语义、条目与门禁状态，不展示实现。
 * 用法：node scripts/contract-review.mjs <id|目录>
 */
import { readFile } from 'node:fs/promises'
import { spawnSync } from 'node:child_process'
import { fileURLToPath } from 'node:url'
import path from 'node:path'
import {
  CATEGORY_ID,
  parseFrontmatter,
  resolveContractDir,
  tableRows,
} from './contract-shared.mjs'

const LINT = path.join(path.dirname(fileURLToPath(import.meta.url)), 'contract-lint.mjs')

function section(body, keyword) {
  const lines = body.split('\n')
  const start = lines.findIndex((line) => line.startsWith('##') && line.includes(keyword))
  if (start === -1) return ''
  const rest = lines.slice(start + 1)
  const end = rest.findIndex((line) => line.startsWith('## '))
  return (end === -1 ? rest : rest.slice(0, end)).join('\n').trim()
}

async function main() {
  const argument = process.argv[2]
  if (!argument) throw new Error('用法：node scripts/contract-review.mjs <id|目录>')
  const dir = resolveContractDir(argument)

  const text = await readFile(path.join(dir, 'contract.md'), 'utf8')
  const { data, body } = parseFrontmatter(text, path.join(dir, 'contract.md'))

  const counts = { E: 0, S: 0, I: 0, B: 0, A: 0 }
  for (const row of tableRows(body)) {
    if (CATEGORY_ID.test(row.cells[0])) counts[row.cells[0][0]] += 1
  }
  const total = Object.values(counts).reduce((sum, value) => sum + value, 0)

  let evidenceRows = 0
  try {
    const evidence = await readFile(path.join(dir, 'evidence.md'), 'utf8')
    evidenceRows = tableRows(evidence.split('\n---\n').slice(1).join('\n---\n')).filter((row) =>
      CATEGORY_ID.test(row.cells[0]),
    ).length
  } catch {
    evidenceRows = 0
  }

  const contractLines = text.trim().split('\n').length
  const lint = spawnSync(process.execPath, [LINT, '--dir', dir], { encoding: 'utf8' })
  const lintPass = lint.status === 0

  console.log(`契约 ${data.id}`)
  console.log(`  状态：${data.status} ｜ 授权：${data.authorization} ｜ 版本：v${data.contract_version}`)
  console.log(`  scope：${(data.scope ?? []).join(' , ')}`)
  console.log(
    `  条目：E${counts.E} S${counts.S} I${counts.I} B${counts.B} A${counts.A}（共 ${total} 条）`,
  )
  console.log(`  人审成本：契约 ${contractLines} 行 + 证据 ${evidenceRows} 行`)
  console.log(`  门禁：lint ${lintPass ? '通过' : '未通过'}`)
  if (!lintPass) {
    const detail = `${lint.stdout ?? ''}${lint.stderr ?? ''}`.trim()
    for (const line of detail.split('\n').filter(Boolean).slice(0, 12)) console.log(`    ${line}`)
  }
  const ambiguity = section(body, '歧义')
  console.log(`  歧义与未决：${ambiguity && ambiguity !== '- 无' ? '见下' : '无'}`)
  if (ambiguity && ambiguity !== '- 无') {
    for (const line of ambiguity.split('\n')) console.log(`    ${line}`)
  }
  console.log(
    lintPass
      ? `\n人审通过后执行：pnpm contract:approve ${data.id}`
      : '\n先补齐契约再 approve（approve 会强制跑 lint）',
  )
}

try {
  await main()
} catch (error) {
  console.error(`contract-review: ${error.message}`)
  process.exitCode = 1
}
