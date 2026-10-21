# pulsar-kernel

Cordis 框架层最小工程：只保留“去掉所有插件后”的字面核心，不含任何 harness 业务能力。

## 范围

包含（9 个 vendored 包，约 6,500 行 TypeScript）：

| 包 | 作用 |
|---|---|
| `cordis` | Context、插件注册表、Fiber 生命周期、事件、服务、日志 |
| `cosmokit` | 基础工具库 |
| `schemastery` | 配置 schema |
| `loader` | 运行时插件树与 Loader 服务 |
| `include` | YAML/JSON 配置 include |
| `group` | Loader 嵌套插件组 |
| `hmr` | 模块热替换 |
| `timer` | 可随 Fiber 销毁的定时器 |
| `logger-console` | console 日志导出器 |

不包含：`core/*`（session、tools、agent、agent-loop、system-prompt、scope）、`llm`、会话持久化、存储、工具、Web/桌面外壳，以及 `.agents`、`docs`、`apps`、`scripts` 等仓库配套。

## 结构

```text
pulsar-kernel/
  vendor/           # 从 deepseek-harness 原样复制，未做修改
  src/
    kernel.ts       # KernelRuntime：装配框架插件，可逆启动/销毁
    index.ts        # 库出口；直接运行时执行最小启动示例
  pnpm-workspace.yaml
  tsconfig.json     # paths 指向 vendor 源码
```

## 命令

```sh
pnpm install
pnpm typecheck
pnpm start
```

`pnpm start` 预期输出：`hello, pulsar-kernel` 及挂载/销毁日志。

## 来源与许可

`vendor/` 复制自 [deepseek-ai/deepseek-harness](https://github.com/deepseek-ai/deepseek-harness) 的 `vendor/` 目录，保留了其本地修改与 `@deepseek-ai/*` 作用域，未做任何改动。各包许可证见其目录内的 `LICENSE`（MIT）。
