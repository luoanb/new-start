// 浏览器侧薄 bootstrap：拉 boot graph、加载插件产物、注册槽位、处理开发期热替换。
// Slots / RPC / Events / Locale 的完整契约实现见 @pulsar/plugin-client。

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
      const produced = entry.component(container, { owner: owner ?? {}, scopeId, ...api })
      // 组件可以自己 append，也可以把节点交回来（框架适配器多为后者）。
      if (produced instanceof Node) container.append(produced)
      else if (Array.isArray(produced)) container.append(...produced.filter((item) => item instanceof Node))
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
    header.textContent = 'pulsar-host'
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

/** 产物自带的 CSS 必须在插件模块执行前挂上，避免首帧无样式。 */
function loadAssets(assets, owned) {
  for (const asset of assets ?? []) {
    if (!asset.endsWith('.css')) continue
    const link = document.createElement('link')
    link.rel = 'stylesheet'
    link.href = asset
    document.head.append(link)
    owned.push(() => link.remove())
  }
}

/** 已加载插件的清理项；热替换时按 id 整体回滚。 */
const loadedPlugins = new Map()

async function loadPlugin(plugin) {
  const disposers = []
  const owned = []
  const scopedSlots = {
    register(spec, component) {
      const off = slots.register(spec, component)
      disposers.push(off)
      return off
    },
    render: slots.render,
    validate: slots.validate,
  }

  try {
    loadAssets(plugin.client.assets, owned)
    const module = await import(new URL(plugin.client.url, location.href).href)
    const apply = module.apply ?? module.default
    if (typeof apply !== 'function') {
      throw new Error(`client plugin ${plugin.id} has no apply export`)
    }
    const cleanup = await apply({
      id: plugin.id,
      slots: scopedSlots,
      api,
      events,
      locale,
    })
    if (typeof cleanup === 'function') disposers.push(cleanup)
    loadedPlugins.set(plugin.id, { disposers, owned })
  } catch (error) {
    for (const dispose of disposers.reverse()) dispose()
    for (const dispose of owned.reverse()) dispose()
    console.error(`client plugin ${plugin.id} failed`, error)
    report(error)
  }
}

function disposePlugin(id) {
  const record = loadedPlugins.get(id)
  if (!record) return
  loadedPlugins.delete(id)
  for (const dispose of record.disposers.reverse()) dispose()
  for (const dispose of record.owned.reverse()) dispose()
}

async function boot() {
  registerShell()

  const graph = await (await fetch('/api/boot')).json()
  for (const plugin of graph.plugins) await loadPlugin(plugin)

  slots.render('root', app, {}, undefined, undefined)

  const orphans = slots.validate()
  if (orphans.length) {
    report(new Error(`注册到了无人声明的槽: ${orphans.join(', ')}`))
  }

  // 开发期热替换：宿主只告知「哪个插件变了」，具体原因由适配器内部处理。
  events.on('plugin/changed', async (payload) => {
    if (!payload || typeof payload.id !== 'string') return
    disposePlugin(payload.id)
    const plugin = (await (await fetch('/api/boot')).json()).plugins.find((item) => item.id === payload.id)
    if (!plugin) return
    await loadPlugin(plugin)
    slots.render('root', app, {}, undefined, undefined)
  })
}

boot().catch(report)
