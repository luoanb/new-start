import type { PluginSource } from '../types.ts'

/** 编译输入：源码描述 + 一个空目录，编译器把全部产物写到 outDir。 */
export interface CompileInput {
  source: PluginSource
  /** 空目录；rename 到 hash 目录由 artifacts 流水线负责。 */
  outDir: string
}

/** 编译结果：文件已落盘，或产物在外部服务上。 */
export type CompileResult =
  | {
    kind: 'files'
    entry: string
    assets: string[]
    /** 参与本次编译的全部源文件（含依赖）及其内容 hash；用于命中判定。 */
    inputs?: Record<string, string>
  }
  | { kind: 'external'; url: string; assets?: string[] }

/**
 * 编译策略：唯一允许认识打包器的接口。
 *
 * 每个框架独立实现；`BrowserRuntime` 只调用它，不认识任何打包器。
 */
export interface Compiler {
  readonly name: string
  /** 该编译器需要的页面级共享依赖。 */
  readonly imports: Record<string, string>
  /** 浏览器侧 runtime 入口；纯服务端适配器不提供。 */
  readonly clientEntry?: string
  /** 构建配置指纹（不含源码内容），参与命中判定。 */
  fingerprint(source: PluginSource): Promise<string>
  compile(input: CompileInput): Promise<CompileResult>
  /** 开发模式可选：监听源码变更，返回停止函数。 */
  watch?(input: CompileInput, onChange: () => void): Promise<() => void>
}
