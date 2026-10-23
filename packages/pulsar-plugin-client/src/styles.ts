import type { StyleSheet } from './types.ts'

/** 样式注入：同一 id 只保留一份 `<style>`，返回的撤销函数幂等。 */
export class DomStyleSheet implements StyleSheet {
  private readonly owned = new Map<string, HTMLStyleElement>()

  constructor(private readonly root: ParentNode = document.head) {}

  add(id: string, css: string): () => void {
    const existing = this.owned.get(id)
    if (existing) existing.textContent = css
    const element = existing ?? this.create(id, css)
    this.owned.set(id, element)
    return () => {
      if (this.owned.get(id) !== element) return
      element.remove()
      this.owned.delete(id)
    }
  }

  /** 移除全部已注入样式。 */
  dispose(): void {
    for (const element of this.owned.values()) element.remove()
    this.owned.clear()
  }

  private create(id: string, css: string): HTMLStyleElement {
    const element = document.createElement('style')
    element.dataset.pulsarStyle = id
    element.textContent = css
    this.root.append(element)
    return element
  }
}
