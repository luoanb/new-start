import path from 'node:path'

import type { Classification } from './types.ts'
import { toPosix } from './util.ts'

export type ClassificationRule = {
  id: string
  pattern: string
}

export function classify(filePath: string, rules: readonly ClassificationRule[]): Classification {
  const normalized = toPosix(filePath)
  const tags = rules
    .filter((rule) => new RegExp(rule.pattern, 'i').test(normalized))
    .map((rule) => rule.id)
  return {
    primary: tags[0] ?? 'unclassified',
    tags,
  }
}

export function workspaceOf(filePath: string): string | null {
  const [root, name] = toPosix(filePath).split('/')
  if (!root || !name) return null
  if (root === 'apps' || root === 'packages' || root === 'demos') return `${root}/${name}`
  return root
}

export function extensionOf(filePath: string): string {
  return path.posix.extname(toPosix(filePath)).toLowerCase()
}
