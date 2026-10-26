import { readFileSync } from 'node:fs'

import ts from 'typescript'

import type { ComplexityFact, ComplexityFunctionFact, FileFact } from './types.ts'
import type { Progress } from './util.ts'

export type ComplexityConfig = {
  decisionNodeKinds: readonly string[]
  logicalOperators: readonly string[]
}

export type ComplexityCoverage = {
  ok: number
  failed: number
  skipped: number
  failures: { path: string; reason: string }[]
}

function decisionName(node: ts.Node, config: ComplexityConfig): string | null {
  if (
    ts.isBinaryExpression(node) &&
    config.logicalOperators.includes(ts.tokenToString(node.operatorToken.kind) ?? '')
  ) {
    return ts.tokenToString(node.operatorToken.kind) ?? 'logical'
  }
  if (ts.isDefaultClause(node)) return null
  const kind = ts.isCaseClause(node) ? 'SwitchCase' : ts.SyntaxKind[node.kind]
  return config.decisionNodeKinds.includes(kind) ? kind : null
}

function nodeName(node: ts.Node): string {
  if (
    (ts.isFunctionDeclaration(node) ||
      ts.isFunctionExpression(node) ||
      ts.isMethodDeclaration(node)) &&
    node.name &&
    ts.isIdentifier(node.name)
  ) {
    return node.name.text
  }
  return ts.isFunctionLike(node) ? '(anonymous)' : ''
}

function functionComplexity(
  node: ts.Node,
  sourceFile: ts.SourceFile,
  config: ComplexityConfig,
  lineOffset: number,
): ComplexityFunctionFact {
  const decisions: string[] = []
  const visit = (current: ts.Node) => {
    const decision = decisionName(current, config)
    if (decision) decisions.push(decision)
    ts.forEachChild(current, visit)
  }
  visit(node)
  const start = sourceFile.getLineAndCharacterOfPosition(node.getStart(sourceFile))
  const end = sourceFile.getLineAndCharacterOfPosition(node.getEnd())
  return {
    name: nodeName(node) || '(anonymous)',
    startLine: start.line + 1 + lineOffset,
    endLine: end.line + 1 + lineOffset,
    cyclomatic: 1 + decisions.length,
    cognitive: cognitiveOf(node, nodeName(node)),
    decisions,
  }
}

function isLoop(
  node: ts.Node,
): node is
  ts.ForStatement | ts.ForInStatement | ts.ForOfStatement | ts.WhileStatement | ts.DoStatement {
  return (
    ts.isForStatement(node) ||
    ts.isForInStatement(node) ||
    ts.isForOfStatement(node) ||
    ts.isWhileStatement(node) ||
    ts.isDoStatement(node)
  )
}

function cognitiveOf(root: ts.Node, name: string): number {
  const visit = (node: ts.Node, nesting: number): number => {
    if (node !== root && ts.isFunctionLike(node)) return 0
    if (ts.isIfStatement(node)) return walkIf(node, nesting, false)
    if (isLoop(node)) return walkLoop(node, nesting)
    if (ts.isSwitchStatement(node)) {
      return 1 + nesting + visit(node.expression, nesting) + visit(node.caseBlock, nesting + 1)
    }
    if (ts.isCatchClause(node)) return 1 + nesting + visit(node.block, nesting + 1)
    if (ts.isConditionalExpression(node)) {
      return (
        1 +
        nesting +
        visit(node.condition, nesting) +
        visit(node.whenTrue, nesting + 1) +
        visit(node.whenFalse, nesting + 1)
      )
    }
    if (ts.isBinaryExpression(node)) {
      const operator = ts.tokenToString(node.operatorToken.kind)
      if (operator === '&&' || operator === '||') {
        return 1 + visit(node.left, nesting) + visit(node.right, nesting)
      }
    }
    let score = 0
    if (
      name &&
      ts.isCallExpression(node) &&
      ts.isIdentifier(node.expression) &&
      node.expression.text === name
    ) {
      score += 1
    }
    ts.forEachChild(node, (child) => {
      score += visit(child, nesting)
    })
    return score
  }
  const walkIf = (statement: ts.IfStatement, nesting: number, elseIf: boolean): number => {
    let score = elseIf ? 1 : 1 + nesting
    score += visit(statement.expression, nesting)
    score += visit(statement.thenStatement, nesting + 1)
    if (!statement.elseStatement) return score
    if (ts.isIfStatement(statement.elseStatement))
      return score + walkIf(statement.elseStatement, nesting, true)
    return score + 1 + visit(statement.elseStatement, nesting + 1)
  }
  const walkLoop = (statement: ts.Node, nesting: number): number => {
    let score = 1 + nesting
    if (ts.isForStatement(statement)) {
      if (statement.initializer) score += visit(statement.initializer, nesting)
      if (statement.condition) score += visit(statement.condition, nesting)
      if (statement.incrementor) score += visit(statement.incrementor, nesting)
      return score + visit(statement.statement, nesting + 1)
    }
    if (ts.isForInStatement(statement) || ts.isForOfStatement(statement)) {
      score += visit(statement.expression, nesting)
      return score + visit(statement.statement, nesting + 1)
    }
    if (ts.isWhileStatement(statement)) {
      score += visit(statement.expression, nesting)
      return score + visit(statement.statement, nesting + 1)
    }
    if (ts.isDoStatement(statement)) {
      score += visit(statement.expression, nesting)
      return score + visit(statement.statement, nesting + 1)
    }
    return score
  }
  return visit(root, 0)
}

function scriptOf(filePath: string, content: string): { code: string; lineOffset: number } {
  if (!filePath.endsWith('.svelte')) return { code: content, lineOffset: 0 }
  const open = /<script\b[^>]*>/i.exec(content)
  if (!open) return { code: '', lineOffset: 0 }
  const start = open.index + open[0].length
  const close = content.indexOf('</script>', start)
  return {
    code: content.slice(start, close === -1 ? content.length : close),
    lineOffset: content.slice(0, start).match(/\n/g)?.length ?? 0,
  }
}

export function measureSource(
  filePath: string,
  content: string,
  config: ComplexityConfig,
): ComplexityFact {
  const script = scriptOf(filePath, content)
  const sourceFile = ts.createSourceFile(
    filePath,
    script.code,
    ts.ScriptTarget.Latest,
    true,
    ts.ScriptKind.TS,
  )
  const found: ts.Node[] = []
  const visit = (node: ts.Node) => {
    if (ts.isFunctionLike(node)) found.push(node)
    ts.forEachChild(node, visit)
  }
  visit(sourceFile)
  const functions = found.map((node) =>
    functionComplexity(node, sourceFile, config, script.lineOffset),
  )
  const moduleCognitive = cognitiveOf(sourceFile, '')
  return {
    functions,
    functionCount: functions.length,
    moduleCognitive,
    templateBranches: filePath.endsWith('.svelte')
      ? (content.match(/\{#(if|each|await)\b/g)?.length ?? 0)
      : null,
    status: 'ok',
    reason: null,
  }
}

function failedComplexity(reason: string): ComplexityFact {
  return {
    functions: [],
    functionCount: 0,
    moduleCognitive: 0,
    templateBranches: null,
    status: 'failed',
    reason,
  }
}

export function collectComplexity(
  repoRoot: string,
  files: readonly FileFact[],
  config: ComplexityConfig,
  onProgress?: Progress,
): ComplexityCoverage {
  const coverage: ComplexityCoverage = { ok: 0, failed: 0, skipped: 0, failures: [] }
  const total = files.filter((file) => ['.ts', '.svelte'].includes(file.extension)).length
  let seen = 0
  onProgress?.(`复杂度 0/${total}`)
  for (const file of files) {
    if (!['.ts', '.svelte'].includes(file.extension)) {
      coverage.skipped += 1
      continue
    }
    seen += 1
    try {
      const content = readFileSync(`${repoRoot}/${file.path}`, 'utf8')
      file.complexity = measureSource(file.path, content, config)
      coverage.ok += 1
    } catch (error) {
      const reason = error instanceof Error ? error.message : 'unknown parse error'
      file.complexity = failedComplexity(reason)
      coverage.failed += 1
      coverage.failures.push({ path: file.path, reason })
    }
    if (seen === total || seen % 250 === 0) onProgress?.(`复杂度 ${seen}/${total}`)
  }
  return coverage
}
