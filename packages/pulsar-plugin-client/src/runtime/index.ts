// 适配器契约入口：零打包器依赖，宿主只引用这里。

export { BrowserRuntime } from './browser.ts'
export type { RuntimeMountHandle } from './browser.ts'
export {
  ensureArtifact,
  prune,
  readLatest,
  readManifest,
  resolveDir,
  serveDir,
  writeAtomic,
} from './artifacts.ts'
export type { Artifact, ArtifactManifest, EnsureArtifactOptions } from './artifacts.ts'
export type {
  CompileInput,
  CompileResult,
  Compiler,
} from './compiler/types.ts'
export type {
  Activation,
  ClientDescriptor,
  Logger,
  PluginObject,
  PluginRuntime,
  PluginSource,
  RouteHandler,
  RuntimeChange,
  RuntimeContext,
} from './types.ts'
