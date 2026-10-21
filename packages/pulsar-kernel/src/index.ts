import { pathToFileURL } from 'node:url'
import { KernelRuntime } from './kernel.ts'

export { KernelRuntime } from './kernel.ts'
export type { KernelOptions } from './kernel.ts'

const entry = process.argv[1]
const isDirectRun = entry !== undefined && import.meta.url === pathToFileURL(entry).href

if (isDirectRun) {
  const kernel = new KernelRuntime()
  await kernel.start()
  await new Promise<void>((resolve) => setTimeout(resolve, 10))
  await kernel.stop()
}
