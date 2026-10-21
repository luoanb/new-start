// hello-plugin 的浏览器侧：注册一个 main 槽组件，并通过 RPC 调用 Host。
// 该文件由 Host 用 esbuild 打成单文件 ESM 后提供给浏览器。

interface ClientApi {
  call(method: string, args?: unknown): Promise<unknown>
}

interface ClientEvents {
  on(type: string, handler: (payload: unknown) => void): () => void
}

interface SlotView {
  scopeId?: string
  renderSlot(name: string, container: HTMLElement, owner?: unknown): void
}

interface SlotSpec {
  name: string
  id: string
  scope?: 'root' | 'scoped' | 'scoped-maybe'
  order?: number
  children?: string[]
}

interface ClientContext {
  id: string
  slots: {
    register(spec: SlotSpec, component: (container: HTMLElement, view: SlotView) => void): () => void
  }
  api: ClientApi
  events: ClientEvents
  locale: {
    t(key: string): string
    messages: Record<string, string>
  }
}

export function apply(ctx: ClientContext) {
  ctx.locale.messages['hello.title'] = 'Hello plugin'
  ctx.locale.messages['hello.idle'] = 'No RPC call yet.'
  ctx.locale.messages['hello.call'] = 'Call host RPC'
  ctx.locale.messages['hello.greeted'] = 'Host events received: 0'

  let greeted = 0
  let output: HTMLParagraphElement | undefined

  ctx.slots.register({ name: 'main', id: 'hello-panel' }, (container, view) => {
    void view

    const article = document.createElement('article')
    const title = document.createElement('h2')
    title.textContent = ctx.locale.t('hello.title')

    output = document.createElement('p')
    output.textContent = ctx.locale.t('hello.idle')

    const button = document.createElement('button')
    button.textContent = ctx.locale.t('hello.call')
    button.addEventListener('click', async () => {
      button.disabled = true
      try {
        const result = await ctx.api.call('hello.greet', { name: 'client' }) as { message?: string }
        output!.textContent = result?.message ?? 'no message'
      } catch (error) {
        output!.textContent = error instanceof Error ? error.message : String(error)
      } finally {
        button.disabled = false
      }
    })

    article.append(title, output, button)
    container.append(article)
  })

  ctx.events.on('hello/greeted', () => {
    greeted += 1
    if (output) output.textContent = `Host events received: ${greeted}`
  })
}
