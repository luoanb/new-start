import { Context, Service } from '@deepseek-ai/cordis'
import ConsoleLogger from '@deepseek-ai/cordis-plugin-logger-console'
import Timer from '@deepseek-ai/cordis-plugin-timer'

export interface KernelOptions {
  /** 是否挂载 console 日志导出器，默认 true */
  logger?: boolean
}

declare module '@deepseek-ai/cordis' {
  interface Context {
    demo: DemoService
  }

  interface Events {
    'kernel/ready'(name: string): void
  }
}

/** 演示服务：仅用于证明服务注册与生命周期可逆。 */
class DemoService extends Service {
  constructor(ctx: Context) {
    super(ctx, 'demo')
  }

  greet(name: string): string {
    return `hello, ${name}`
  }
}

/** 演示插件：依赖 demo 服务，注册事件监听与 effect。 */
const demoPlugin = Object.assign((ctx: Context) => {
  ctx.on('kernel/ready', (name) => {
    ctx.logger.info('%s', ctx.demo.greet(name))
  })

  ctx.effect(() => {
    ctx.logger.info('demo plugin mounted')
    return () => ctx.logger.info('demo plugin disposed')
  }, 'demo plugin')
}, {
  inject: ['demo'],
})

/**
 * 最小 Cordis 运行时。
 *
 * 只装配框架层插件（日志导出器、定时器）与一个内置演示插件，
 * 不包含任何 harness 业务能力。
 */
export class KernelRuntime {
  readonly root: Context
  private readonly options: KernelOptions
  private started = false

  constructor(options: KernelOptions = {}) {
    this.root = new Context()
    this.options = options
  }

  /** 启动运行时；重复调用幂等。 */
  async start(): Promise<void> {
    if (this.started) return

    if (this.options.logger !== false) {
      await this.root.plugin(ConsoleLogger)
    }
    await this.root.plugin(Timer)
    await this.root.plugin(DemoService)
    await this.root.plugin(demoPlugin)

    this.started = true
    this.root.emit('kernel/ready', 'pulsar-kernel')
  }

  /** 销毁插件树，释放全部 effect 与监听器。 */
  async stop(): Promise<void> {
    if (!this.started) return
    await this.root.fiber.dispose()
    this.started = false
  }
}
