import type { Locale } from './types.ts'

/** 本地化词典：缺词回落为 key，不抛错。 */
export class LocaleDictionary implements Locale {
  readonly messages: Record<string, string> = {}

  constructor(readonly current = 'zh-CN') {}

  t(key: string): string {
    return this.messages[key] ?? key
  }
}
