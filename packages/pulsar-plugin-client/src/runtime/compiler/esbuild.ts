import { createHash } from 'node:crypto'
import { mkdir, readFile, writeFile } from 'node:fs/promises'
import { dirname, relative, resolve, sep } from 'node:path'
import { build, context, type BuildOptions, type Loader, type Plugin } from 'esbuild'
import type { PluginSource } from '../types.ts'
import type { CompileInput, CompileResult, Compiler } from './types.ts'

export interface EsbuildCompilerOptions {
  /** 框架插件工厂；每次 build 重新创建实例。 */
  plugins?: () => Plugin[]
  /** 额外的扩展名 loader，例如 `{ '.vue': 'text' }`。 */
  loader?: Record<string, Loader>
  jsx?: 'automatic' | 'preserve'
  jsxImportSource?: string
  /** 由 import map 提供的共享依赖，不进入产物。 */
  external: string[]
  define?: Record<string, string>
  /** 该编译器声明给宿主的页面级共享依赖。 */
  imports: Record<string, string>
  /** 浏览器侧 runtime 入口 URL。 */
  clientEntry: string
  /** 输出文件名（不含扩展名），默认 `client`。 */
  entryName?: string
  target?: string
}

/**
 * 基于 esbuild 的 `Compiler` 组合件。
 *
 * 不认识任何框架；框架差异（loader / jsx / 插件）全部由 options 注入。
 */
export function esbuildCompiler(options: EsbuildCompilerOptions): Compiler {
  const entryName = options.entryName ?? 'client'
  const target = options.target ?? 'es2022'

  return {
    name: 'esbuild',
    imports: options.imports,
    clientEntry: options.clientEntry,

    async fingerprint(_source: PluginSource): Promise<string> {
      return createHash('sha256')
        .update(JSON.stringify({
          entryName,
          target,
          loader: options.loader ?? {},
          jsx: options.jsx ?? null,
          jsxImportSource: options.jsxImportSource ?? null,
          external: [...options.external].sort(),
          define: options.define ?? {},
          plugins: options.plugins?.().map((plugin) => plugin.name).sort() ?? [],
        }))
        .digest('hex')
        .slice(0, 16)
    },

    async compile(input: CompileInput): Promise<CompileResult> {
      const result = await build(toBuildOptions(input))
      const outputFiles = result.outputFiles ?? []
      const files = await Promise.all(outputFiles.map(async (file) => {
        const name = relative(input.outDir, file.path).split(sep).join('/')
        await mkdir(dirname(file.path), { recursive: true })
        await writeFile(file.path, file.contents)
        return name
      }))

      // 入口产物固定为 `<entryName>.js`；其余全部按资源投递（CSS、字体、图片）。
      const entry = files.find((name) => name === `${entryName}.js`)
        ?? files.find((name) => name.endsWith('.js'))
        ?? files[0]
      if (!entry) {
        throw new Error(`esbuild produced no output for ${input.source.entry}`)
      }
      return {
        kind: 'files',
        entry,
        assets: files.filter((name) => name !== entry),
        inputs: await collectInputs(result.metafile?.inputs ?? {}),
      }
    },

    /** 开发期监听：编译成功就回调，重建与落盘仍由 `ensureArtifact()` 负责。 */
    async watch(input: CompileInput, onChange: () => void): Promise<() => void> {
      const base = toBuildOptions(input)
      const ctx = await context({
        ...base,
        plugins: [
          ...(base.plugins ?? []),
          {
            name: 'pulsar-watch-notify',
            setup(builder) {
              builder.onEnd((result) => {
                if (result.errors.length === 0) onChange()
              })
            },
          },
        ],
      })
      await ctx.watch()
      return () => void ctx.dispose()
    },
  }

  function toBuildOptions(input: CompileInput): BuildOptions {
    return {
      entryPoints: [{ in: input.source.entry, out: entryName }],
      outdir: input.outDir,
      bundle: true,
      write: false,
      format: 'esm',
      platform: 'browser',
      target,
      logLevel: 'silent',
      metafile: true,
      external: options.external,
      assetNames: '[name]-[hash]',
      ...(options.loader ? { loader: options.loader } : {}),
      ...(options.jsx ? { jsx: options.jsx } : {}),
      ...(options.jsxImportSource ? { jsxImportSource: options.jsxImportSource } : {}),
      ...(options.define ? { define: options.define } : {}),
      ...(options.plugins ? { plugins: options.plugins() } : {}),
    }
  }
}

/**
 * 把 esbuild 的依赖图转成「相对路径 -> 内容 hash」。
 *
 * 只读源码文件；虚拟模块（`<...>`）不参与命中判定。
 */
async function collectInputs(inputs: Record<string, unknown>): Promise<Record<string, string>> {
  const result: Record<string, string> = {}
  for (const file of Object.keys(inputs)) {
    if (file.startsWith('<')) continue
    const absolute = resolve(file)
    try {
      const content = await readFile(absolute)
      result[relative(process.cwd(), absolute).split(sep).join('/')] =
        createHash('sha256').update(content).digest('hex')
    } catch {
      // 依赖已消失时跳过；下次编译会重新计算。
    }
  }
  return result
}
