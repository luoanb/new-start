// 浏览器侧插件运行时：加载 boot graph、提供泛化 Slots、RPC 与事件订阅。
// 这个文件由 Host 直接以静态资源提供，不参与 TypeScript 编译。

const app = document.querySelector('#app')

const slots = (() => {
  const registry = new Map()

  function register(spec, component) {
    if (!spec || typeof spec.name !== 'string' || typeof spec.id !== 'string') {
      throw new Error('invalid slot registration')
    }
    if (typeof component !== 'function') {
      throw new Error(`slot ${spec.id} has no component`)
    }
    if (registry.has(spec.id)) {
      throw new Error(`duplicate slot id: ${spec.id}`)
    }
    const entry = { spec, component }
    registry.set(spec.id, entry)
    return () => {
      if (registry.get(spec.id) === entry) registry.delete(spec.id)
    }
  }

  function scopeMatches(scope, scopeId) {
    if (!scope || scope === 'root') return true
    if (scope === 'scoped') return scopeId !== undefined
    if (scope === 'scoped-maybe') return true
    throw new Error(`unknown slot scope: ${scope}`)
  }

  function render(name, container, owner, scopeId, parent) {
    const declared = parent ? parent.spec.children ?? [] : ['root']
    if (!declared.includes(name)) {
      throw new Error(`slot not declared by parent: ${name}`)
    }

    const cells = [...registry.values()]
      .filter((entry) => entry.spec.name === name && scopeMatches(entry.spec.scope, scopeId))
      .sort((a, b) => (a.spec.order ?? 0) - (b.spec.order ?? 0))

    for (const entry of cells) {
      const api = {
        renderSlot: (child, node, childOwner, options) => {
          const nextScope = options && 'scopeId' in options ? options.scopeId : scopeId
          render(child, node, childOwner ?? {}, nextScope, entry)
        },
      }
      entry.component(container, { owner: owner ?? {}, scopeId, ...api })
    }
  }

  /** 返回注册到了无人声明槽位的 id；用于启动后的可见告警。 */
  function validate() {
    const declared = new Set(['root'])
    for (const entry of registry.values()) {
      for (const child of entry.spec.children ?? []) declared.add(child)
    }
    return [...registry.values()]
      .filter((entry) => !declared.has(entry.spec.name))
      .map((entry) => entry.spec.id)
  }

  return { register, render, validate }
})()

const api = {
  async call(method, args) {
    const response = await fetch('/api/rpc', {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ method, args }),
    })
    const body = await response.json()
    if (!response.ok) throw new Error(body.error ?? `rpc failed: ${method}`)
    return body.result
  },
}

const events = (() => {
  const handlers = new Map()
  const source = new EventSource('/api/events')
  source.onmessage = (event) => {
    try {
      const { type, payload } = JSON.parse(event.data)
      for (const handler of handlers.get(type) ?? []) handler(payload)
    } catch (error) {
      console.error('invalid event frame', error)
    }
  }
  return {
    on(type, handler) {
      const list = handlers.get(type) ?? []
      list.push(handler)
      handlers.set(type, list)
      return () => {
        handlers.set(type, (handlers.get(type) ?? []).filter((item) => item !== handler))
      }
    },
  }
})()

// locale 机制保留，词典初始为空；t() 在缺词时回落为 key。
const locale = {
  current: 'zh-CN',
  messages: {},
  t(key) {
    return this.messages[key] ?? key
  },
}

function registerShell() {
  slots.register({ name: 'root', id: 'shell', children: ['main'] }, (container, view) => {
    container.replaceChildren()
    const header = document.createElement('header')
    header.textContent = 'pulsar-plugin-host'
    const main = document.createElement('main')
    container.append(header, main)
    view.renderSlot('main', main, {})
  })
}

function report(error) {
  const box = document.createElement('article')
  box.style.borderColor = '#e8463a'
  const title = document.createElement('h2')
  title.textContent = '插件加载失败'
  const text = document.createElement('pre')
  text.textContent = error instanceof Error ? error.stack ?? error.message : String(error)
  box.append(title, text)
  app.append(box)
}

async function boot() {
  registerShell()

  const graph = await (await fetch('/api/boot')).json()
  for (const plugin of graph.plugins) {
    try {
      const url = new URL(plugin.clientUrl, location.href)
      url.searchParams.set('rev', graph.rev)
      const module = await import(url.href)
      const apply = module.apply ?? module.default
      if (typeof apply !== 'function') {
        throw new Error(`client plugin ${plugin.id} has no apply export`)
      }
      await apply({ id: plugin.id, slots, api, events, locale })
    } catch (error) {
      console.error(`client plugin ${plugin.id} failed`, error)
      report(error)
    }
  }

  slots.render('root', app, {}, undefined, undefined)

  const orphans = slots.validate()
  if (orphans.length) {
    report(new Error(`注册到了无人声明的槽: ${orphans.join(', ')}`))
  }
}

boot().catch(report)
