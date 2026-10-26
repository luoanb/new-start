import { readFileSync, statSync } from 'node:fs'

import { type ClassificationRule, classify, extensionOf, workspaceOf } from './classify.ts'
import type { FileFact, SizeFact } from './types.ts'
import { toPosix } from './util.ts'

export type FileInput = {
  path: string
  tracked: boolean
}

function countLines(
  content: string,
): Pick<SizeFact, 'totalLines' | 'nonEmptyLines' | 'lineEnding'> {
  if (content.length === 0) return { totalLines: 0, nonEmptyLines: 0, lineEnding: 'none' }
  const normalized = content.endsWith('\n') ? content.slice(0, -1) : content
  const lines = normalized.split(/\r?\n/)
  const hasCrLf = content.includes('\r\n')
  const hasLf = content.replaceAll('\r\n', '').includes('\n')
  return {
    totalLines: lines.length,
    nonEmptyLines: lines.filter((line) => line.trim().length > 0).length,
    lineEnding: hasCrLf && hasLf ? 'mixed' : hasCrLf ? 'crlf' : hasLf ? 'lf' : 'none',
  }
}

export function inspectFile(
  repoRoot: string,
  input: FileInput,
  rules: readonly ClassificationRule[],
  binaryExtensions: readonly string[],
): FileFact {
  const relativePath = toPosix(input.path)
  const absolutePath = `${repoRoot}/${relativePath}`
  const extension = extensionOf(relativePath)
  const size: SizeFact = {
    bytes: 0,
    totalLines: null,
    nonEmptyLines: null,
    lineEnding: 'none',
    binary: binaryExtensions.includes(extension),
    readable: false,
  }
  try {
    const stat = statSync(absolutePath)
    size.bytes = stat.size
    size.readable = stat.isFile()
    if (size.readable && !size.binary) {
      const content = readFileSync(absolutePath)
      if (content.includes(0)) {
        size.binary = true
      } else {
        Object.assign(size, countLines(content.toString('utf8')))
      }
    }
  } catch {
    size.readable = false
  }
  return {
    path: relativePath,
    extension,
    workspace: workspaceOf(relativePath),
    classification: classify(relativePath, rules),
    tracked: input.tracked,
    size,
    git: {},
    complexity: null,
  }
}
