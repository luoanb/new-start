import { run } from './run.ts'
import { reportExitCode } from './schema.ts'
import { runGit, toPosix } from './util.ts'

try {
  const repoRoot = toPosix(runGit(process.cwd(), ['rev-parse', '--show-toplevel']).trim())
  const report = await run(repoRoot, process.argv.slice(2))
  process.exitCode = reportExitCode(report)
} catch (error) {
  console.error(error instanceof Error ? error.message : String(error))
  process.exitCode = 1
}
